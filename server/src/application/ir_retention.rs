//! IR 取证表保留策略（T009）：`ir_snapshots` 按每主机条数封顶、`ir_page_cache` 按 TTL。
//!
//! **为什么不跟 `HELM_RETENTION_DAYS` 走**：IR 数据是取证资产（基线对比、差异取证），
//! 按时间一刀切会把「唯一的一份基线」也删掉；两张表形态也不同：
//!
//! - `ir_snapshots`：每主机可有多条（label + findings JSONB），**无上限增长** →
//!   按「每主机保留最近 N 条」封顶（`HELM_IR_SNAPSHOT_KEEP_PER_AGENT`，默认 20）。
//! - `ir_page_cache`：以 `(agent_id, kind)` 为主键、写入即 `created_at = now()`（upsert），
//!   行数有界（主机数 × kind 数）→ 治的是**陈旧内容**与已注销主机的死缓存，
//!   按 TTL 清理（`HELM_IR_PAGE_CACHE_TTL_DAYS`，默认 30）。
//!
//! **磁盘预算口径**（写进 `deploy/README.md` §3）：占用 ≈ Σ主机 (快照数 × 单快照体积)
//! 加上 主机数 × kind 数 × 单缓存体积。单快照体积由 findings 条目数决定，
//! 可用 `pg_total_relation_size('ir_snapshots')` 实测校准。
//!
//! 结构照 `application::retention`：`run_once` 可测，`spawn` 只管 24h 周期。

use std::time::Duration;

use chrono::Utc;

use crate::store::Db;
use crate::store::ir_repo;

/// 清理周期：每 24h 一轮。
pub const SWEEP_INTERVAL: Duration = Duration::from_secs(24 * 3600);

/// 单轮清理结果（日志与测试断言用）。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct IrRetentionReport {
    /// 超出「每主机保留条数」被删的快照行数
    pub snapshots: u64,
    /// 超 TTL 未刷新的页面缓存行数
    pub page_cache: u64,
}

impl IrRetentionReport {
    pub fn total(&self) -> u64 {
        self.snapshots + self.page_cache
    }
}

/// 单轮清理（pub 供集成测试驱动，不经 24h 周期）。
///
/// 两项配置都支持「0 = 不清理」；某一步失败只记 warn 并记 0，不影响另一步。
pub async fn run_once(db: &Db, keep_per_agent: i64, ttl_days: i64) -> IrRetentionReport {
    let snapshots = match ir_repo::delete_excess_snapshots(db.pool(), keep_per_agent).await {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(error = %e, "ir retention: 快照超额清理失败");
            0
        }
    };

    let page_cache = if ttl_days <= 0 {
        tracing::debug!(ttl_days, "ir retention: 页面缓存 TTL 非正数，跳过");
        0
    } else {
        let cutoff = Utc::now() - chrono::Duration::days(ttl_days);
        match ir_repo::delete_stale_page_cache(db.pool(), cutoff).await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = %e, "ir retention: 页面缓存 TTL 清理失败");
                0
            }
        }
    };

    IrRetentionReport {
        snapshots,
        page_cache,
    }
}

/// 后台清理任务（`lib.rs` 阶段 5 挂载）。
pub fn spawn(db: Db, keep_per_agent: i64, ttl_days: i64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(SWEEP_INTERVAL).await;
            let r = run_once(&db, keep_per_agent, ttl_days).await;
            if r.total() > 0 {
                tracing::info!(
                    snapshots_deleted = r.snapshots,
                    page_cache_deleted = r.page_cache,
                    keep_per_agent,
                    ttl_days,
                    "ir retention cleanup"
                );
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn total_sums_both_dimensions() {
        let r = IrRetentionReport {
            snapshots: 3,
            page_cache: 4,
        };
        assert_eq!(r.total(), 7);
        assert_eq!(IrRetentionReport::default().total(), 0);
    }
}
