//! 数据保留清理（C1 + T008）：按 `HELM_RETENTION_DAYS` 清理时序类与历史表。
//!
//! 覆盖面：metrics / alerts / notifications / 已吊销 api_keys / jobs / audit_logs /
//! file_transfers / **status_events（T008 新增）**。
//!
//! **不在此列**：IR 取证表（`ir_snapshots` / `ir_page_cache`）——取证数据的保留口径由
//! T009 的显式策略决定（见 `application::ir_retention`）。
//!
//! 结构照 `job_sweeper`：`run_once` 是可被集成测试直接驱动的单轮动作，`spawn` 只管周期。

use std::time::Duration;

use chrono::Utc;
use uuid::Uuid;

use crate::store::Db;

/// 清理周期：每 24h 一轮。
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// 单轮清理结果（日志字段与测试断言用）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RetentionReport {
    pub metrics: u64,
    pub alerts: u64,
    pub notifications: u64,
    pub api_keys: u64,
    pub jobs: u64,
    pub audit_logs: u64,
    pub file_transfers: u64,
    pub status_events: u64,
}

impl RetentionReport {
    /// 本轮删除总行数（测试断言 / 日志汇总用）。
    pub fn total(&self) -> u64 {
        self.metrics
            + self.alerts
            + self.notifications
            + self.api_keys
            + self.jobs
            + self.audit_logs
            + self.file_transfers
            + self.status_events
    }
}

/// 单表清理的容错包装（返回行数）：某张表失败不影响本轮其它表，错误必须可见。
fn rows(res: sqlx::Result<u64>, table: &'static str) -> u64 {
    match res {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(table, error = %e, "retention cleanup failed");
            0
        }
    }
}

/// 同上，用于返回 id 列表的仓库（jobs）。
fn rows_of(res: sqlx::Result<Vec<Uuid>>, table: &'static str) -> u64 {
    match res {
        Ok(v) => v.len() as u64,
        Err(e) => {
            tracing::warn!(table, error = %e, "retention cleanup failed");
            0
        }
    }
}

/// 单轮清理（pub 供集成测试驱动，不经 24h 周期）。
///
/// `retention_days` 复用 `HELM_RETENTION_DAYS`（默认 90）；**非正数是危险配置**——会把
/// 全部历史一次清空，故显式告警（不改既有语义，只让风险可见）。
pub async fn run_once(db: &Db, retention_days: i64) -> RetentionReport {
    if retention_days <= 0 {
        tracing::warn!(
            retention_days,
            "HELM_RETENTION_DAYS 非正数：本轮会删除全部历史数据（含 status_events）——请确认是预期"
        );
    }
    let cutoff = Utc::now() - chrono::Duration::days(retention_days);

    RetentionReport {
        metrics: rows(
            crate::store::metric_repo::MetricRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "metrics",
        ),
        alerts: rows(
            crate::store::alert_repo::AlertRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "alerts",
        ),
        notifications: rows(
            crate::store::notification_repo::NotificationRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "notifications",
        ),
        api_keys: rows(
            crate::store::api_key_repo::ApiKeyRepo::new(db.clone())
                .delete_revoked_before(cutoff)
                .await,
            "api_keys",
        ),
        jobs: rows_of(
            crate::store::job_repo::JobRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "jobs",
        ),
        audit_logs: rows(
            crate::store::audit_repo::AuditRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "audit_logs",
        ),
        file_transfers: rows(
            crate::store::file_transfer_repo::FileTransferRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "file_transfers",
        ),
        status_events: rows(
            crate::store::status_event_repo::StatusEventRepo::new(db.clone())
                .delete_before(cutoff)
                .await,
            "status_events",
        ),
    }
}

/// 后台清理任务（`lib.rs` 阶段 5 挂载）。
pub fn spawn(db: Db, retention_days: i64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            let r = run_once(&db, retention_days).await;
            tracing::info!(
                metrics_deleted = r.metrics,
                alerts_deleted = r.alerts,
                notifications_deleted = r.notifications,
                api_keys_deleted = r.api_keys,
                jobs_deleted = r.jobs,
                audit_deleted = r.audit_logs,
                file_transfers_deleted = r.file_transfers,
                status_events_deleted = r.status_events,
                retention_days,
                "retention cleanup"
            );
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_sums_all_dimensions() {
        let r = RetentionReport {
            metrics: 1,
            alerts: 2,
            notifications: 3,
            api_keys: 4,
            jobs: 5,
            audit_logs: 6,
            file_transfers: 7,
            status_events: 8,
        };
        assert_eq!(r.total(), 36);
        assert_eq!(RetentionReport::default().total(), 0);
    }
}
