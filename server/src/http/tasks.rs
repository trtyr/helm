//! 任务下发端点：脚本执行 + 定时任务。

use crate::application::exec_service::ExecService;
use crate::application::scheduler;
use crate::http::AppState;
use crate::store::task_repo::TaskRepo;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::{Value, json};

/// 脚本执行（复用 exec：command 可为 `sh -c "..."` 等）。
#[derive(Debug, Deserialize)]
pub struct ScriptBody {
    pub agent_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 定时任务。
#[derive(Debug, Deserialize)]
pub struct ScheduleBody {
    pub agent_id: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub interval_secs: u64,
}

/// 脚本下发：POST /api/v1/tasks/script
pub async fn run_script(
    State(state): State<AppState>,
    Json(body): Json<ScriptBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let service = ExecService::new(state.db, state.registry);
    match service
        .exec(&body.agent_id, &body.command, &body.args)
        .await
    {
        Ok(job_id) => Ok(Json(json!({ "job_id": job_id }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// 定时任务：POST /api/v1/tasks/schedule
pub async fn schedule(
    State(state): State<AppState>,
    Json(body): Json<ScheduleBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let task = TaskRepo::new(state.db.clone())
        .create(
            &body.command,
            "schedule",
            json!({
                "agent_id": body.agent_id.clone(),
                "command": body.command.clone(),
                "args": body.args.clone(),
                "interval_secs": body.interval_secs,
            }),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": e.to_string() })),
            )
        })?;

    let exec = ExecService::new(state.db, state.registry);
    scheduler::schedule(
        task.id,
        exec,
        body.agent_id,
        body.command,
        body.args,
        body.interval_secs,
    );

    Ok(Json(json!({ "task_id": task.id })))
}
