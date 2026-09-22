//! 重启对账（T007）：把上一进程遗留的在飞状态对齐到**诚实的终态**。
//!
//! **为什么需要**：进程崩溃或被重启时，在飞的 job 行仍停在 `running`/`queued`、
//! 文件传输停在 `pending`/`transferring`。此前这些行只能等 `HELM_JOB_TIMEOUT_SECS`
//! （默认 300s）被 job sweeper 标成 `timed_out`——那是**假超时**：命令随进程一起消失
//! 了，不是它自己超时。本模块在启动瞬间完成对账，并把真实原因记下来。
//!
//! **调用时机（有语义）**：必须早于 `spawn_background_tasks`——否则上一进程遗留的
//! 超龄 running 行会先被 job sweeper 判成 `timed_out`，对账就失去了意义。
//!
//! **不改状态枚举**：终态复用既有 `failed`（DB CHECK 仅有
//! queued/running/succeeded/failed/timed_out/cancelled），原因写进 `output`。

use crate::store::Db;
use crate::store::file_transfer_repo::FileTransferRepo;
use crate::store::job_repo::JobRepo;

/// 写进 job `output` 的原因文案（控制台任务详情可见，故用可读中文）。
pub const ORPHAN_REASON: &str =
    "[server restarted] 服务重启前未完成，已置 failed（非超时：命令随进程终止）";

/// 对账结果（日志与测试断言用）。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReconcileReport {
    /// 被对齐的 job 数（原 running/queued）
    pub jobs: usize,
    /// 被对齐的文件传输数（原 pending/transferring）
    pub transfers: usize,
}

/// 启动期对账：把上一进程遗留的非终态行对齐为 `failed`。
pub async fn reconcile_orphans(db: &Db) -> anyhow::Result<ReconcileReport> {
    let jobs = JobRepo::new(db.clone()).fail_orphans(ORPHAN_REASON).await?;
    for id in &jobs {
        tracing::warn!(job_id = %id, "重启对账：遗留 job 置 failed（服务重启前未完成）");
    }

    let transfers = FileTransferRepo::new(db.clone()).fail_orphans().await?;
    for id in &transfers {
        tracing::warn!(transfer_id = %id, "重启对账：遗留文件传输置 failed（服务重启前未完成）");
    }

    let report = ReconcileReport {
        jobs: jobs.len(),
        transfers: transfers.len(),
    };
    if report.jobs > 0 || report.transfers > 0 {
        tracing::warn!(
            jobs = report.jobs,
            transfers = report.transfers,
            "重启对账完成：上一进程遗留的在飞状态已对齐为 failed"
        );
    }
    Ok(report)
}
