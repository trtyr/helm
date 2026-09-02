//! 审计查询端点。

use crate::application::audit_service::AuditService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct AuditQuery {
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

/// 列出审计记录：GET /api/v1/audit?page=&limit=
pub async fn list_audit(
    State(state): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, Error> {
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let rows = AuditService::new(state.db)
        .list_paged(q.limit.max(1), offset)
        .await?;
    Ok(Json(json!({ "audit": rows })))
}
