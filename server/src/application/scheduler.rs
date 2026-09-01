//! 应用层：定时任务调度与持久化恢复。

use std::time::Duration;

use crate::application::exec_service::ExecService;
use crate::domain::Result;
use crate::store::{Db, task_repo::TaskRepo};
use uuid::Uuid;

/// 启动定时任务循环：每 `interval_secs` 秒向 Agent 下发一次命令。
/// `task_id` 为已落库的 task 记录 id（持久化标识）。
pub fn schedule(
    task_id: Uuid,
    exec: ExecService,
    agent_id: String,
    command: String,
    args: Vec<String>,
    interval_secs: u64,
) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(interval_secs.max(1)));
        // 首个 tick 立即触发
        interval.tick().await;
        loop {
            interval.tick().await;
            match exec.exec(&agent_id, &command, &args).await {
                Ok(job_id) => {
                    tracing::info!(task_id = %task_id, job_id = %job_id, "scheduled exec ok")
                }
                Err(e) => {
                    tracing::warn!(task_id = %task_id, error = %e, "scheduled exec failed")
                }
            }
        }
    });
}

/// 从 tasks 表恢复定时任务调度（Server 重启后调用）。
pub async fn resume_scheduled(db: Db, exec: ExecService) -> Result<()> {
    let tasks = TaskRepo::new(db).list_scheduled().await?;
    for task in tasks {
        let agent_id = task
            .params
            .get("agent_id")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let command = task
            .params
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let args = task
            .params
            .get("args")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let interval = task
            .params
            .get("interval_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60);

        if agent_id.is_empty() || command.is_empty() {
            tracing::warn!(task_id = %task.id, "skipping invalid scheduled task");
            continue;
        }
        schedule(task.id, exec.clone(), agent_id, command, args, interval);
        tracing::info!(task_id = %task.id, "resumed scheduled task");
    }
    Ok(())
}
