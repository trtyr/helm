//! 命令执行 HTTP 端点。

use crate::application::exec_service::ExecService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct ExecBody {
    pub agent_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 下发命令：POST /api/v1/exec
pub async fn exec(
    State(state): State<AppState>,
    Json(body): Json<ExecBody>,
) -> Result<Json<Value>, Error> {
    let service = ExecService::new(state.db, state.registry);
    let job_id = service
        .exec(&body.agent_id, &body.command, &body.args)
        .await?;
    Ok(Json(json!({ "job_id": job_id })))
}
