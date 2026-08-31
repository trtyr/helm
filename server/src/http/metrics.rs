//! 指标查询端点。

use crate::domain::Error;
use crate::http::AppState;
use crate::store::metric_repo::MetricRepo;
use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    pub host_id: Uuid,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

/// 查询指标：GET /api/v1/metrics?host_id=...&limit=...
pub async fn list_metrics(
    State(state): State<AppState>,
    Query(q): Query<MetricsQuery>,
) -> Result<Json<Value>, Error> {
    let metrics = MetricRepo::new(state.db).recent(q.host_id, q.limit).await?;
    Ok(Json(json!({ "metrics": metrics })))
}
