//! 通知中心端点（决策 009：系统内部小卡片——上线/下线/预警）。

use crate::application::notification_service::NotificationService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct NotificationQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// 只看未读（"true" / "1"）。
    #[serde(default)]
    pub unread: Option<String>,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    50
}

/// 列出通知：GET /api/v1/notifications?page=&limit=&unread=
pub async fn list_notifications(
    State(state): State<AppState>,
    Query(q): Query<NotificationQuery>,
) -> Result<Json<Value>, Error> {
    let unread_only = matches!(q.unread.as_deref(), Some("true") | Some("1"));
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let rows = NotificationService::new(state.db, state.streams)
        .list_paged(q.limit.max(1), offset, unread_only)
        .await?;
    Ok(Json(json!({ "notifications": rows })))
}

/// 未读数：GET /api/v1/notifications/unread-count
pub async fn unread_count(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let count = NotificationService::new(state.db, state.streams)
        .unread_count()
        .await?;
    Ok(Json(json!({ "count": count })))
}

/// 标记单条已读：POST /api/v1/notifications/{id}/read
pub async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let hit = NotificationService::new(state.db, state.streams)
        .mark_read(id)
        .await?;
    if !hit {
        return Err(Error::NotFound(format!("notification: {id}")));
    }
    Ok(Json(json!({ "ok": true })))
}

/// 全部标记已读：POST /api/v1/notifications/read-all
pub async fn mark_all_read(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let updated = NotificationService::new(state.db, state.streams)
        .mark_all_read()
        .await?;
    Ok(Json(json!({ "updated": updated })))
}
