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
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    50
}

/// 列出告警：GET /api/v1/alerts?page=&limit=
pub async fn list_alerts(
    State(state): State<AppState>,
    Query(q): Query<AlertQuery>,
) -> Result<Json<Value>, Error> {
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let rows = AlertService::new(state.db)
        .list_paged(q.limit.max(1), offset)
        .await?;
    Ok(Json(json!({ "alerts": rows })))
}
