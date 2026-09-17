//! Job 超时兜底扫描（EN-64 超时 + EN-67 queued 孤行）。
//!
//! 主路径是 agent 侧超时（ExecRequest.timeout_secs 杀进程上报）；本 sweeper 兜底
//! 「agent 未限时（旧版/未透传）」「queued 孤行（下发时 agent 离线）」两类漏网：
//! - running 超龄 → 置 `timed_out`，agent 在线时顺带补发 JobCancel（杀残留进程）；
//! - queued 超龄 → 置 `failed`（从未送达，无进程可杀）。
//!
//! 模式对照 `notification_service::spawn_offline_sweeper`。

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::store::Db;
use crate::store::job_repo::JobRepo;
use helm_proto::pb::{JobCancel, ServerMessage, server_message};

/// 扫描周期（固定 30s；阈值可配 `HELM_JOB_TIMEOUT_SECS`，0 = 禁用）。
pub const SWEEP_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

/// 单次扫描结果（测试断言用）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SweepReport {
    /// 本轮置 timed_out 的 running job 数
    pub expired_running: usize,
    /// 本轮置 failed 的 queued 孤行数
    pub expired_queued: usize,
    /// 其中 agent 在线、已补发 JobCancel 的数量
    pub cancels_sent: usize,
}

/// 单轮扫描（pub 供集成测试直接驱动，不经 30s 周期）。
pub async fn sweep_once(
    db: &Db,
    connections: &ConnectionRegistry,
    timeout_secs: u64,
) -> SweepReport {
    let mut report = SweepReport::default();
    if timeout_secs == 0 {
        return report; // 0 = 禁用
    }
    let repo = JobRepo::new(db.clone());

    match repo.expire_running(timeout_secs as i64).await {
        Ok(expired) => {
            report.expired_running = expired.len();
            for job in expired {
                // 在线 agent 补发取消：job 已终态，此处只为杀掉目标机上的残留进程
                if let Some(agent_id) = job.agent_id
                    && connections.is_online(&agent_id).await
                {
                    let msg = ServerMessage {
                        kind: Some(server_message::Kind::JobCancel(JobCancel {
                            job_id: job.id.to_string(),
                        })),
                    };
                    if connections.send(&agent_id, msg).await.is_ok() {
                        report.cancels_sent += 1;
                    }
                }
                tracing::warn!(job_id = %job.id, timeout_secs, "job sweeper: running job timed out");
            }
        }
        Err(e) => tracing::warn!(error = %e, "job sweeper: expire_running failed"),
    }

    match repo.fail_stale_queued(timeout_secs as i64).await {
        Ok(ids) => {
            report.expired_queued = ids.len();
            for id in ids {
                tracing::warn!(job_id = %id, timeout_secs, "job sweeper: stale queued job failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "job sweeper: fail_stale_queued failed"),
    }

    report
}

/// 后台兜底扫描（lib.rs 启动时挂载；`job_timeout_secs == 0` 时不启动）。
pub fn spawn_job_sweeper(db: Db, connections: ConnectionRegistry, timeout_secs: u64) {
    if timeout_secs == 0 {
        tracing::info!("job sweeper disabled (HELM_JOB_TIMEOUT_SECS=0)");
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            let report = sweep_once(&db, &connections, timeout_secs).await;
            if report.expired_running > 0 || report.expired_queued > 0 {
                tracing::info!(
                    timed_out = report.expired_running,
                    queued_failed = report.expired_queued,
                    cancels_sent = report.cancels_sent,
                    "job sweeper pass"
                );
            }
        }
    });
}
