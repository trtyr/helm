pub mod auth;
pub mod error;
pub mod exec;
pub mod files;
pub mod forward;
pub mod health;
pub mod hosts;
pub mod jobs;
pub mod metrics;
pub mod tasks;

use crate::config::Config;
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use axum::Router;
use axum::middleware;
use axum::routing::{get, post};

/// HTTP 层共享状态。
#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub registry: ConnectionRegistry,
    pub transfers: TransferRegistry,
    pub jwt_secret: String,
}

/// 启动 HTTP 服务（控制台 API + health）。
pub async fn serve(
    config: Config,
    db: Db,
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
) -> anyhow::Result<()> {
    let state = AppState {
        db,
        registry,
        transfers,
        jwt_secret: config.jwt_secret.clone(),
    };

    // 受保护路由（需 JWT）
    let protected = Router::new()
        .route("/hosts", get(hosts::list_hosts))
        .route("/exec", post(exec::exec))
        .route("/jobs/{id}", get(jobs::get_job))
        .route("/metrics", get(metrics::list_metrics))
        .route("/files/upload", post(files::upload))
        .route("/files/download", post(files::download))
        .route("/tasks/script", post(tasks::run_script))
        .route("/tasks/schedule", post(tasks::schedule))
        .route("/forward/exec", post(forward::exec))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    let app = Router::new()
        .route("/healthz", get(health::healthz))
        .route("/api/v1/auth/login", post(auth::login))
        .nest("/api/v1", protected)
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(&config.http_addr).await?;
    tracing::info!(addr = %config.http_addr, "http listening");

    axum::serve(listener, app).await?;
    Ok(())
}
