//! 主机查询端点。

use crate::http::AppState;
use crate::store::host_repo::HostRepo;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde_json::{Value, json};

/// 列出主机：GET /api/v1/hosts
pub async fn list_hosts(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let hosts = HostRepo::new(state.db)
        .list()
        .await
        .map_err(|e| internal(e.to_string()))?;
    Ok(Json(json!({ "hosts": hosts })))
}

fn internal(msg: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}
