//! 常驻服务端点：创建 / 列表 / 启停 / 重启 / 日志。

use crate::application::service_service::ServiceService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct CreateServiceBody {
    pub agent_id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub restart_policy: String,
}

fn service(state: &AppState) -> ServiceService {
    ServiceService::new(state.db.clone(), state.registry.clone())
}

/// 创建服务：POST /api/v1/services
pub async fn create_service(
    State(state): State<AppState>,
    Json(body): Json<CreateServiceBody>,
) -> Result<Json<Value>, Error> {
    let row = service(&state)
        .create(
            &body.agent_id,
            &body.name,
            &body.command,
            &body.args,
            &body.restart_policy,
        )
        .await?;
    Ok(Json(json!({ "service": row })))
}

/// 列出服务：GET /api/v1/services
pub async fn list_services(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let rows = service(&state).list().await?;
    Ok(Json(json!({ "services": rows })))
}

/// 启动服务：POST /api/v1/services/{id}/start
pub async fn start_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).start(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 停止服务：POST /api/v1/services/{id}/stop
pub async fn stop_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).stop(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 重启服务：POST /api/v1/services/{id}/restart
pub async fn restart_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).restart(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 查服务日志：GET /api/v1/services/{id}/logs
pub async fn service_logs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let row = service(&state).get(id).await?;
    Ok(Json(json!({ "log": row.log })))
}
