//! 任务下发端点：脚本执行 + 定时任务。

use crate::application::exec_service::ExecService;
use crate::application::scheduler;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::task_repo::TaskRepo;
use axum::Json;
use axum::extract::State;
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
) -> Result<Json<Value>, Error> {
    let service = ExecService::new(state.db, state.registry);
    let job_id = service
        .exec(&body.agent_id, &body.command, &body.args)
        .await?;
    Ok(Json(json!({ "job_id": job_id })))
}

/// 定时任务：POST /api/v1/tasks/schedule
pub async fn schedule(
    State(state): State<AppState>,
    Json(body): Json<ScheduleBody>,
) -> Result<Json<Value>, Error> {
    let task = TaskRepo::new(state.db.clone())
        .create(
            &body.command,
            "exec",
            json!({
                "agent_id": body.agent_id.clone(),
                "command": body.command.clone(),
                "args": body.args.clone(),
                "interval_secs": body.interval_secs,
            }),
        )
        .await?;

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
