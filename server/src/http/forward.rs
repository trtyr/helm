//! 正向连接端点。

use crate::application::forward_service::ForwardService;
use crate::domain::Error;
use axum::Json;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct ForwardBody {
    pub agent_addr: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 正向连接执行命令：POST /api/v1/forward/exec
pub async fn exec(Json(body): Json<ForwardBody>) -> Result<Json<Value>, Error> {
    let service = ForwardService::new();
    let (output, exit_code) = service
        .exec(&body.agent_addr, &body.command, &body.args)
        .await?;
    Ok(Json(json!({
        "output": output,
        "exit_code": exit_code,
    })))
}
