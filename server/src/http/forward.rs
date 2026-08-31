//! 正向连接端点。

use crate::application::forward_service::ForwardService;
use axum::Json;
use axum::http::StatusCode;
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
pub async fn exec(Json(body): Json<ForwardBody>) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let service = ForwardService::new();
    match service
        .exec(&body.agent_addr, &body.command, &body.args)
        .await
    {
        Ok((output, exit_code)) => Ok(Json(json!({
            "output": output,
            "exit_code": exit_code,
        }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}
