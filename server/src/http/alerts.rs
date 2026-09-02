//! 告警查询端点。

use crate::application::alert_service::AlertService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct AlertQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// 列出告警：GET /api/v1/alerts?limit=50
pub async fn list_alerts(
    State(state): State<AppState>,
    Query(q): Query<AlertQuery>,
) -> Result<Json<Value>, Error> {
    let rows = AlertService::new(state.db).list(q.limit).await?;
    Ok(Json(json!({ "alerts": rows })))
}
