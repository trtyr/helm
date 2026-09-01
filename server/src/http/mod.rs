pub mod agents;
pub mod auth;
pub mod error;
pub mod exec;
pub mod files;
pub mod forward;
pub mod health;
pub mod hosts;
pub mod jobs;
pub mod listeners;
pub mod metrics;
pub mod process;
pub mod services;
pub mod tasks;
pub mod terminal;

use crate::config::Config;
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::listener_registry::ListenerRegistry;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use axum::Router;
use axum::middleware;
use axum::routing::{delete, get, post};

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
    pub jwt_secret: String,
    pub server_token: String,
    pub heartbeat_timeout_secs: u64,
    pub session_idle_timeout_secs: u64,
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
) -> anyhow::Result<()> {
    let state = AppState {
        db,
        registry,
        transfers,
        listeners,
        sessions,
        file_list,
        query,
        jwt_secret: config.jwt_secret.clone(),
        server_token: config.server_token.clone(),
        heartbeat_timeout_secs: config.heartbeat_timeout_secs,
        session_idle_timeout_secs: config.session_idle_timeout_secs,
    };

    // 受保护路由（需 JWT）
    let protected = Router::new()
        .route("/hosts", get(hosts::list_hosts).post(hosts::create_host))
        .route("/hosts/{id}/tags", post(hosts::set_host_tags))
        .route("/exec", post(exec::exec))
        .route("/jobs/{id}", get(jobs::get_job))
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
        .route("/agents", get(agents::list_agents))
        .route("/agents/{id}", delete(agents::deregister_agent))
        .route("/agents/{id}/uninstall", post(agents::uninstall_agent))
        .route(
            "/services",
            get(services::list_services).post(services::create_service),
        )
        .route("/services/{id}/start", post(services::start_service))
        .route("/services/{id}/stop", post(services::stop_service))
        .route("/services/{id}/restart", post(services::restart_service))
        .route("/services/{id}/logs", get(services::service_logs))
        .route("/processes/list", post(process::list_processes))
        .route("/processes/kill", post(process::kill_process))
        .route("/net/info", post(process::net_info))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let app = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/api/v1/auth/login", post(auth::login))
        .route("/api/v1/agents/{id}/terminal", get(terminal::terminal))
        .nest("/api/v1", protected)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.http_addr).await?;
    tracing::info!(addr = %config.http_addr, "http listening");

    axum::serve(listener, app).await?;
    Ok(())
}
