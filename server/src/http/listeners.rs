//! 监听器端点。

use crate::application::listener_service::ListenerService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateListenerBody {
    pub name: String,
    pub addr: String,
    #[serde(default = "default_proto")]
    pub proto: String,
    #[serde(default)]
    pub auth: String,
}

fn default_proto() -> String {
    "grpc".to_string()
}

fn service(state: &AppState) -> ListenerService {
    ListenerService::new(
        state.db.clone(),
        state.listeners.clone(),
        state.registry.clone(),
        state.transfers.clone(),
        state.server_token.clone(),
    )
}

/// 创建监听器：POST /api/v1/listeners
pub async fn create_listener(
    State(state): State<AppState>,
    Json(body): Json<CreateListenerBody>,
) -> Result<Json<Value>, Error> {
    let listener = service(&state)
        .create(&body.name, &body.addr, &body.proto, &body.auth)
        .await?;
    Ok(Json(json!({ "listener": listener })))
}

/// 列出监听器：GET /api/v1/listeners
pub async fn list_listeners(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let listeners = service(&state).list().await?;
    Ok(Json(json!({ "listeners": listeners })))
}

/// 启动监听器：POST /api/v1/listeners/{id}/start
pub async fn start_listener(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).start(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 停止监听器：POST /api/v1/listeners/{id}/stop
pub async fn stop_listener(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).stop(id).await?;
    Ok(Json(json!({ "ok": true })))
}
