//! 日志查询端点（P003 T1）：状态事件 GET /api/v1/logs/events。

use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::Value;

use crate::domain::Error;
use crate::http::AppState;
use crate::store::parse_sort;
use crate::store::status_event_repo::StatusEventRepo;

#[derive(Deserialize)]
pub struct EventQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    pub sort: Option<String>,
    pub q: Option<String>,
    pub host_id: Option<String>,
    /// 事件类型过滤（online | offline）
    pub event: Option<String>,
    /// RFC3339 起始时间（含）。
    pub from: Option<String>,
    /// RFC3339 结束时间（含）。
    pub to: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

fn parse_time(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&chrono::Utc))
}

/// GET /api/v1/logs/events：状态事件（上下线/断连原因）分页查询（P003 T1）。
pub async fn list_events(
    State(state): State<AppState>,
    Query(q): Query<EventQuery>,
) -> Result<Json<Value>, Error> {
    let limit = q.limit.clamp(1, 100);
    let offset = (q.page.max(1) - 1) * limit;
    let sort = parse_sort(q.sort.as_ref());
    let from = q.from.as_deref().and_then(parse_time);
    let to = q.to.as_deref().and_then(parse_time);
    let repo = StatusEventRepo::new(state.db);
    let rows = repo
        .list_paged(
            q.q.as_deref(),
            q.host_id.as_deref(),
            q.event.as_deref(),
            from,
            to,
            sort,
            limit,
            offset,
        )
        .await?;
    let total = repo
        .count_filtered(
            q.q.as_deref(),
            q.host_id.as_deref(),
            q.event.as_deref(),
            from,
            to,
        )
        .await?;
    Ok(Json(serde_json::json!({ "events": rows, "total": total })))
}
