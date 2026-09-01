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

/// 解析定时任务参数（纯函数，便于测试）。
pub fn parse_schedule_params(
    params: &serde_json::Value,
) -> Option<(String, String, Vec<String>, u64)> {
    let agent_id = params.get("agent_id")?.as_str()?.to_string();
    let command = params.get("command")?.as_str()?.to_string();
    let args = params
        .get("args")
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let interval = params.get("interval_secs")?.as_u64()?;
    Some((agent_id, command, args, interval))
}

/// 从 tasks 表恢复定时任务调度（Server 重启后调用）。
pub async fn resume_scheduled(db: Db, exec: ExecService) -> Result<()> {
    let tasks = TaskRepo::new(db).list_scheduled().await?;
    for task in tasks {
        let Some((agent_id, command, args, interval)) = parse_schedule_params(&task.params) else {
            tracing::warn!(task_id = %task.id, "skipping invalid scheduled task");
            continue;
        };
        if agent_id.is_empty() || command.is_empty() {
            tracing::warn!(task_id = %task.id, "skipping scheduled task with empty fields");
            continue;
        }
        schedule(task.id, exec.clone(), agent_id, command, args, interval);
        tracing::info!(task_id = %task.id, "resumed scheduled task");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_schedule_params_valid() {
        let p =
            json!({"agent_id": "a1", "command": "echo", "args": ["x", "y"], "interval_secs": 5});
        let (agent, cmd, args, interval) = parse_schedule_params(&p).unwrap();
        assert_eq!(agent, "a1");
        assert_eq!(cmd, "echo");
        assert_eq!(args, vec!["x", "y"]);
        assert_eq!(interval, 5);
    }

    #[test]
    fn parse_schedule_params_missing_fields() {
        assert!(parse_schedule_params(&json!({})).is_none());
        assert!(parse_schedule_params(&json!({"agent_id": "a1"})).is_none());
        assert!(parse_schedule_params(&json!({"agent_id": "a1", "command": "echo"})).is_none());
    }
}
