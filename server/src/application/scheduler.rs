//! 应用层：定时任务调度。

use std::time::Duration;

use crate::application::exec_service::ExecService;
use uuid::Uuid;

/// 创建定时任务：每 `interval_secs` 秒向 Agent 下发一次命令，返回 task_id。
pub fn schedule(
    exec: ExecService,
    agent_id: String,
    command: String,
    args: Vec<String>,
    interval_secs: u64,
) -> Uuid {
    let task_id = Uuid::new_v4();
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
    task_id
}
