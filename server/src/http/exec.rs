//! 命令执行 HTTP 端点。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::application::exec_service::ExecService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Extension, State};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct ExecBody {
    pub agent_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// 可选执行超时（秒）：透传 agent，超时杀进程并以 timed_out 上报。
    pub timeout_secs: Option<u32>,
}

/// 下发命令：POST /api/v1/exec
pub async fn exec(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<ExecBody>,
) -> Result<Json<Value>, Error> {
    let _ = AuditService::new(state.db.clone())
        .record(
            &claims.sub,
            "exec",
            &body.agent_id,
            json!({ "command": body.command }),
        )
        .await;
    let service = ExecService::new(state.db, state.registry);
    let job_id = service
        .exec(&body.agent_id, &body.command, &body.args, body.timeout_secs)
        .await?;
    Ok(Json(json!({ "job_id": job_id })))
}
