//! 主机查询端点。

use crate::domain::Error;
use crate::http::AppState;
use crate::store::host_repo::HostRepo;
use axum::Json;
use axum::extract::State;
use serde_json::{Value, json};

/// 列出主机：GET /api/v1/hosts
pub async fn list_hosts(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let hosts = HostRepo::new(state.db).list().await?;
    Ok(Json(json!({ "hosts": hosts })))
}
