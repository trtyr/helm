pub mod agent_gen;
pub mod agents;
pub mod alerts;
pub mod api_keys;
pub mod audit;
pub mod auth;
pub mod cert;
pub mod client_ip;
pub mod error;
pub mod exec;
pub mod files;
pub mod forward;
pub mod health;
pub mod hosts;
pub mod ir;
pub mod ir_ops;
pub mod jobs;
pub mod listeners;
pub mod logs;
pub mod mcp;
pub mod metrics;
pub mod notifications;
pub mod p2;
pub mod process;
pub mod proxies;
pub mod services;
pub mod stream;
pub mod tasks;
pub mod terminal;

use crate::application::cert_service::CertService;
use crate::config::Config;
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::listener_registry::ListenerRegistry;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use axum::Router;
use axum::middleware;
use axum::routing::{get, post};

mod mime;
mod routes;

use mime::mime_of;
use routes::{exec_routes, host_routes, ir_routes, meta_routes, service_routes};

/// HTTP 层共享状态。
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub registry: ConnectionRegistry,
    pub transfers: TransferRegistry,
    pub listeners: ListenerRegistry,
    pub sessions: SessionRegistry,
    pub file_list: FileListRegistry,
    pub query: QueryRegistry,
    pub streams: StreamRegistry,
    /// 指标落库队列（E2）：listeners 管理页重启监听器时需重建 gRPC 服务。
    pub metrics: crate::application::metric_sink::MetricSink,
    pub jwt_secret: String,
    /// 可接受的 Agent token 全集（A2：主 token + 轮换中的额外 token）
    pub server_tokens: Vec<String>,
    pub heartbeat_timeout_secs: u64,
    pub session_idle_timeout_secs: u64,
    pub cert: CertService,
    pub agent_gen: crate::application::agent_generator::AgentGenService,
    pub proxy_service: crate::application::proxy_service::ProxyService,
    pub conn_registry: ConnectionRegistry,
    /// HTTP 监听端口（MCP loopback 自调用用）。
    pub http_port: u16,
    /// MCP 渐进分层（P002 T4）：tools/list 描述按 tier 收缩。
    pub mcp_tier: u8,
    /// 登录失败计数与退避（A4）：账号 + 来源 IP 两维度，内存态
    pub login_guard: std::sync::Arc<crate::application::login_guard::LoginGuard>,
    /// EN-14：可信代理网段（解析后的 CIDR 列表）；空 = 不信任任何 XFF
    pub trusted_cidrs: std::sync::Arc<Vec<crate::http::client_ip::Cidr>>,
}

/// HTTP 层启动依赖（G2：收口参数爆炸）。
///
/// 调用方（`lib.rs::run`）一次性装配；HTTP 层不再接受裸参数列表。
pub struct HttpServeDeps {
    pub config: Config,
    pub db: Db,
    pub registry: ConnectionRegistry,
    pub transfers: TransferRegistry,
    pub listeners: ListenerRegistry,
    pub sessions: SessionRegistry,
    pub file_list: FileListRegistry,
    pub query: QueryRegistry,
    pub streams: StreamRegistry,
    /// 指标落库队列（E2）：listeners 管理页重启监听器时需重建 gRPC 服务。
    pub metrics: crate::application::metric_sink::MetricSink,
    pub cert: CertService,
    /// 停机协调器（T006）：HTTP 面与 gRPC 监听器共用同一信号源。
    pub shutdown: crate::shutdown::Shutdown,
}

/// 启动 HTTP 服务（控制台 API + health）。
pub async fn serve(deps: HttpServeDeps) -> anyhow::Result<()> {
    let addr = deps.config.http_addr.clone();
    let web_dist = resolve_web_dist(&deps.config)?;
    // 先取出停机句柄（`build_state` 会按值消费 deps）
    let shutdown = deps.shutdown.clone();
    let state = build_state(deps);
    let app = public_routes(&state, web_dist.clone());

    let Some(dist) = web_dist else {
        return serve_http(app, &addr, &shutdown).await;
    };
    // 前后端一体化（HELM_WEB_DIST_DIR）：同一端口托管控制台静态资源 + SPA fallback。
    // API 语义保留：/api、/mcp、/healthz 未匹配仍返回 404，不落回 index.html。
    tracing::info!(dist = %dist.display(), "serving console static files");
    let app = app.fallback(move |req| web_static(dist.clone(), req));
    serve_http(app, &addr, &shutdown).await
}

/// 带优雅停机地跑 HTTP 服务（两个分支共用）。
///
/// 停机语义（T006）：信号到达 → axum 停止接收新连接并排空在飞请求；
/// 长连接（SSE / 终端 WS）若在 [`crate::shutdown::DRAIN_TIMEOUT`] 内没排空，
/// 由兜底路径强制结束，避免进程被无限钉住。
async fn serve_http(
    app: Router,
    addr: &str,
    shutdown: &crate::shutdown::Shutdown,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(addr = %addr, "http listening");

    // A4：登录限速需要来源 IP → make-service 必须带 connect info
    let graceful = shutdown.clone();
    let server = axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(async move { graceful.wait().await });

    tokio::select! {
        r = server => {
            r?;
            tracing::info!("http 面已排空退出");
        }
        _ = shutdown.drain_deadline() => {
            tracing::warn!(
                drain_timeout_secs = crate::shutdown::DRAIN_TIMEOUT.as_secs(),
                "排空超时：仍有长连接未收尾，强制结束 http 面"
            );
        }
    }
    Ok(())
}

/// 组装 HTTP 层共享状态。
fn build_state(deps: HttpServeDeps) -> AppState {
    let config = &deps.config;
    AppState {
        db: deps.db.clone(),
        registry: deps.registry.clone(),
        transfers: deps.transfers,
        listeners: deps.listeners,
        sessions: deps.sessions,
        file_list: deps.file_list,
        query: deps.query,
        streams: deps.streams,
        metrics: deps.metrics,
        jwt_secret: config.jwt_secret.clone(),
        server_tokens: config.accepted_server_tokens(),
        heartbeat_timeout_secs: config.heartbeat_timeout_secs,
        session_idle_timeout_secs: config.session_idle_timeout_secs,
        mcp_tier: config.mcp_tier,
        login_guard: std::sync::Arc::new(crate::application::login_guard::LoginGuard::new()),
        trusted_cidrs: std::sync::Arc::new(crate::http::client_ip::parse_cidr_list(
            &config.trusted_proxy_cidrs,
        )),
        cert: deps.cert,
        agent_gen: crate::application::agent_generator::AgentGenService::new(
            config.agent_source_dir.clone(),
        ),
        proxy_service: crate::application::proxy_service::ProxyService::new(),
        conn_registry: deps.registry,
        http_port: config
            .http_addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
    }
}

/// 解析并校验前端静态资源目录（HELM_WEB_DIST_DIR）：GET /mcp 回 MCP 页依赖它。
fn resolve_web_dist(config: &Config) -> anyhow::Result<Option<std::path::PathBuf>> {
    if config.web_dist_dir.is_empty() {
        return Ok(None);
    }
    let d = std::path::PathBuf::from(&config.web_dist_dir);
    if !d.join("index.html").is_file() {
        return Err(anyhow::anyhow!(
            "web_dist_dir {:?} 不存在 index.html（先构建 console：pnpm --dir console build）",
            d
        ));
    }
    Ok(Some(d))
}

/// 根路由：公开端点 + `/api/v1` 受保护组。
fn public_routes(state: &AppState, web_dist: Option<std::path::PathBuf>) -> Router {
    let dist = web_dist.clone();
    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/mcp", {
            // GET → console 的 MCP 页（前端路由 deep link，需 HELM_WEB_DIST_DIR）；POST → JSON-RPC。
            post(mcp::mcp).get(move || async move {
                use axum::response::IntoResponse;
                match dist.as_ref() {
                    Some(d) => mcp_console(d.clone()).await,
                    None => {
                        (axum::http::StatusCode::METHOD_NOT_ALLOWED, "not found").into_response()
                    }
                }
            })
        })
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/agents/{id}/terminal", get(terminal::terminal))
        .route(
            "/api/v1/services/{id}/logs/stream",
            get(stream::service_logs_stream),
        )
        .route("/api/v1/services/stream", get(stream::services_stream))
        .route("/api/v1/jobs/{id}/stream", get(stream::job_stream))
        .route("/api/v1/metrics/stream", get(stream::metrics_stream))
        .route(
            "/api/v1/notifications/stream",
            get(stream::notifications_stream),
        )
        .route(
            "/api/v1/ir/memscan/{id}/stream",
            get(stream::memscan_stream),
        )
        .route("/api/v1/agents/cert", post(cert::issue_cert))
        .nest("/api/v1", protected_routes(state))
        .with_state(state.clone())
}

/// 受保护路由（`/api/v1` 分组）：统一挂 JWT/API-key 中间件。
fn protected_routes(state: &AppState) -> Router<AppState> {
    host_routes()
        .merge(exec_routes())
        .merge(service_routes())
        .merge(ir_routes())
        .merge(meta_routes())
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ))
}

/// 请求路径 → `dist` 下的安全相对路径（P006 P0-1：防目录穿越）。
///
/// 只接受纯 `Normal` 分量：绝对路径、`..`、以及含 `\` 的路径一律拒绝——**反斜杠必须单独拦**，
/// 因为在 Unix 上 `..\..\etc` 是**单个** `Normal` 分量（不是 `ParentDir`），只查分量会漏。
/// 空路径按 SPA 根处理（`index.html`）。
fn safe_rel_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("index.html".to_string());
    }
    if path.contains('\\') {
        return None;
    }
    let p = std::path::Path::new(path);
    if p.is_absolute()
        || p.components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_string())
}

/// 前后端一体化的静态托管 fallback：命中文件直接回，未命中回 index.html（SPA 路由）。
async fn web_static(
    dist: std::path::PathBuf,
    req: axum::extract::Request,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let path = req.uri().path().trim_start_matches('/').to_string();
    if path.starts_with("api/") || path == "mcp" || path == "healthz" {
        // API 未匹配路径保持 JSON 404 语义，不落回 SPA
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    }
    // 防目录穿越（P006 P0-1）：hyper 不规范化请求目标里的 `..`，直接 join 会逃出 web 根。
    let Some(rel) = safe_rel_path(&path) else {
        return (axum::http::StatusCode::NOT_FOUND, "not found").into_response();
    };
    let file = dist.join(&rel);
    let serve = |bytes: axum::body::Bytes, mime: &'static str, immutable: bool| {
        axum::response::Response::builder()
            .status(axum::http::StatusCode::OK)
            .header(axum::http::header::CONTENT_TYPE, mime)
            .header(
                axum::http::header::CACHE_CONTROL,
                if immutable {
                    "public, max-age=31536000, immutable"
                } else {
                    "no-cache"
                },
            )
            .body(axum::body::Body::from(bytes))
            .unwrap()
    };
    if tokio::fs::metadata(&file)
        .await
        .map(|m| m.is_file())
        .unwrap_or(false)
        && let Ok(bytes) = tokio::fs::read(&file).await
    {
        return serve(bytes.into(), mime_of(&rel), rel.starts_with("assets/"));
    }
    // SPA fallback：前端 history 路由刷新回 index.html
    match tokio::fs::read(dist.join("index.html")).await {
        Ok(bytes) => serve(bytes.into(), "text/html; charset=utf-8", false),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// GET /mcp → console 的 MCP 页面（前端 /mcp 路由 deep link；JSON-RPC 只走 POST）。
async fn mcp_console(dist: std::path::PathBuf) -> axum::response::Response {
    use axum::response::IntoResponse;
    match tokio::fs::read(dist.join("index.html")).await {
        Ok(bytes) => axum::response::Response::builder()
            .status(axum::http::StatusCode::OK)
            .header(axum::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
            .header(axum::http::header::CACHE_CONTROL, "no-cache")
            .body(axum::body::Body::from(bytes))
            .unwrap(),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

#[cfg(test)]
mod safe_rel_path_tests {
    use super::safe_rel_path;

    #[test]
    fn accepts_normal_asset_paths() {
        assert_eq!(safe_rel_path("").as_deref(), Some("index.html"));
        assert_eq!(
            safe_rel_path("assets/app.js").as_deref(),
            Some("assets/app.js")
        );
        assert_eq!(safe_rel_path("index.html").as_deref(), Some("index.html"));
    }

    #[test]
    fn rejects_parent_dir_traversal() {
        assert!(safe_rel_path("../../etc/passwd").is_none());
        assert!(safe_rel_path("assets/../../etc/passwd").is_none());
        assert!(safe_rel_path("..").is_none());
    }

    #[test]
    fn rejects_backslash_and_absolute() {
        // Unix 下 `..\..\etc` 只有一个 Normal 分量：必须靠 `\` 检查拦住
        assert!(safe_rel_path("..\\..\\etc\\passwd").is_none());
        assert!(safe_rel_path("assets\\app.js").is_none());
        assert!(safe_rel_path("/etc/passwd").is_none());
    }
}
