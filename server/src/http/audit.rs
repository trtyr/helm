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
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

/// 列出审计记录：GET /api/v1/audit?limit=50
pub async fn list_audit(
    State(state): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<Json<Value>, Error> {
    let rows = AuditService::new(state.db).list(q.limit).await?;
    Ok(Json(json!({ "audit": rows })))
}
