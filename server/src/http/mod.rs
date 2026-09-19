pub mod agent_gen;
pub mod agents;
pub mod alerts;
pub mod api_keys;
pub mod audit;
pub mod auth;
pub mod cert;
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
use axum::routing::{delete, get, post, put};

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
    pub server_token: String,
    pub heartbeat_timeout_secs: u64,
    pub session_idle_timeout_secs: u64,
    pub cert: CertService,
    pub agent_gen: crate::application::agent_generator::AgentGenService,
    pub proxy_service: crate::application::proxy_service::ProxyService,
    pub conn_registry: ConnectionRegistry,
    /// HTTP 监听端口（MCP loopback 自调用用）。
    pub http_port: u16,
}

/// 启动 HTTP 服务（控制台 API + health）。
#[allow(clippy::too_many_arguments)]
pub async fn serve(
    config: Config,
    db: Db,
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
    listeners: ListenerRegistry,
    sessions: SessionRegistry,
    file_list: FileListRegistry,
    query: QueryRegistry,
    streams: StreamRegistry,
    metrics: crate::application::metric_sink::MetricSink,
    cert: CertService,
) -> anyhow::Result<()> {
    let state = AppState {
        db: db.clone(),
        registry: registry.clone(),
        transfers,
        listeners,
        sessions,
        file_list,
        query,
        streams,
        metrics,
        jwt_secret: config.jwt_secret.clone(),
        server_token: config.server_token.clone(),
        heartbeat_timeout_secs: config.heartbeat_timeout_secs,
        session_idle_timeout_secs: config.session_idle_timeout_secs,
        cert,
        agent_gen: crate::application::agent_generator::AgentGenService::new(
            config.agent_source_dir.clone(),
        ),
        proxy_service: crate::application::proxy_service::ProxyService::new(),
        conn_registry: registry,
        http_port: config
            .http_addr
            .rsplit(':')
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(8080),
    };

    // 受保护路由（需 JWT）
    let protected = Router::new()
        .route("/hosts", get(hosts::list_hosts).post(hosts::create_host))
        .route(
            "/hosts/{id}",
            get(hosts::get_host)
                .put(hosts::update_host)
                .delete(hosts::delete_host),
        )
        .route("/hosts/{id}/tags", post(hosts::set_host_tags))
        .route("/exec", post(exec::exec))
        .route("/jobs", get(jobs::list_jobs))
        .route("/jobs/{id}", get(jobs::get_job))
        .route("/jobs/{id}/cancel", post(jobs::cancel_job))
        .route("/metrics", get(metrics::list_metrics))
        .route("/files/upload", post(files::upload))
        .route("/files/download", post(files::download))
        .route("/files/list", post(files::list))
        .route("/tasks/script", post(tasks::run_script))
        .route("/tasks/schedule", post(tasks::schedule))
        .route("/forward/exec", post(forward::exec))
        .route(
            "/listeners",
            get(listeners::list_listeners).post(listeners::create_listener),
        )
        .route("/listeners/{id}/start", post(listeners::start_listener))
        .route("/listeners/{id}/stop", post(listeners::stop_listener))
        .route(
            "/listeners/{id}",
            put(listeners::update_listener).delete(listeners::delete_listener),
        )
        .route("/agents", get(agents::list_agents))
        .route(
            "/agents/{id}",
            get(agents::get_agent).delete(agents::deregister_agent),
        )
        .route("/agents/{id}/tags", put(agents::update_agent_tags))
        .route("/agents/{id}/uninstall", post(agents::uninstall_agent))
        .route(
            "/agent-gen",
            get(agent_gen::list_jobs).post(agent_gen::create),
        )
        .route("/agent-gen/{id}", get(agent_gen::get_job))
        .route("/agent-gen/{id}/download", get(agent_gen::download))
        .route(
            "/services",
            get(services::list_services).post(services::create_service),
        )
        .route("/services/{id}/start", post(services::start_service))
        .route("/services/{id}/stop", post(services::stop_service))
        .route("/services/{id}/restart", post(services::restart_service))
        .route("/services/{id}/logs", get(services::service_logs))
        .route(
            "/services/{id}",
            put(services::update_service).delete(services::delete_service),
        )
        .route("/processes/list", post(process::list_processes))
        .route("/processes/kill", post(process::kill_process))
        .route("/net/info", post(process::net_info))
        .route("/sys-services/list", post(process::list_sys_services))
        .route("/sys-services/action", post(process::sys_service_action))
        .route("/ir/scan", post(ir::ir_scan))
        .route("/ir/memscan", post(ir::mem_scan))
        .route("/ir/memscan/stream", post(ir_ops::memscan_stream_start))
        .route("/ir/cache", get(ir::get_cache))
        .route("/ir/fs-timeline", post(p2::fs_timeline))
        .route("/ir/evidence", post(p2::evidence))
        .route("/exec/batch", post(p2::batch_exec))
        .route("/ir/autorun-action", post(ir_ops::autorun_action))
        .route(
            "/ir/snapshots",
            post(ir_ops::create_snapshot).get(ir_ops::list_snapshots),
        )
        .route("/ir/snapshots/compare", post(ir_ops::compare_snapshots))
        .route(
            "/ir/snapshots/{id}",
            get(ir_ops::get_snapshot).delete(ir_ops::delete_snapshot),
        )
        .route(
            "/proxies",
            get(proxies::list_proxies).post(proxies::create_proxy),
        )
        .route("/proxies/{id}", delete(proxies::stop_proxy))
        .route("/audit", get(audit::list_audit))
        .route("/alerts", get(alerts::list_alerts))
        .route("/notifications", get(notifications::list_notifications))
        .route(
            "/notifications/unread-count",
            get(notifications::unread_count),
        )
        .route("/notifications/{id}/read", post(notifications::mark_read))
        .route(
            "/notifications/read-all",
            post(notifications::mark_all_read),
        )
        .route("/api-keys", get(api_keys::list).post(api_keys::create))
        .route(
            "/api-keys/{id}",
            get(api_keys::get_one).delete(api_keys::revoke),
        )
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", post(auth::change_password))
        .route("/auth/change-username", post(auth::change_username))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let app = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/mcp", post(mcp::mcp))
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
        .nest("/api/v1", protected)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.http_addr).await?;
    tracing::info!(addr = %config.http_addr, "http listening");

    if config.web_dist_dir.is_empty() {
        axum::serve(listener, app).await?;
    } else {
        // 前后端一体化（HELM_WEB_DIST_DIR）：同一端口托管控制台静态资源 + SPA fallback。
        // API 语义保留：/api、/mcp、/healthz 未匹配仍返回 404，不落回 index.html。
        let dist = std::path::PathBuf::from(&config.web_dist_dir);
        if !dist.join("index.html").is_file() {
            return Err(anyhow::anyhow!(
                "web_dist_dir {:?} 不存在 index.html（先构建 console：pnpm --dir console build）",
                dist
            ));
        }
        tracing::info!(dist = %dist.display(), "serving console static files");
        let app = app.fallback(move |req| web_static(dist.clone(), req));
        axum::serve(listener, app).await?;
    }
    Ok(())
}

/// 静态资源 MIME（Vite 产物常用类型，覆盖不全时浏览器按猜测处理也无碍）。
fn mime_of(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
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
    let rel = if path.is_empty() { "index.html" } else { &path };
    let file = dist.join(rel);
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
        return serve(bytes.into(), mime_of(rel), path.starts_with("assets/"));
    }
    // SPA fallback：前端 history 路由刷新回 index.html
    match tokio::fs::read(dist.join("index.html")).await {
        Ok(bytes) => serve(bytes.into(), "text/html; charset=utf-8", false),
        Err(_) => (axum::http::StatusCode::NOT_FOUND, "not found").into_response(),
    }
}
