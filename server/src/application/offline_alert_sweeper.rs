//! 离线升级告警 sweeper（P003 T2）：主机离线持续超阈值 → 升级为 alerts。
//!
//! 与 notification_service::spawn_offline_sweeper（半开死连接清理）语义互补：
//! 那个管「断连即发的 offline 通知」，本 sweeper 管「离线持续 N 分钟还没回来」的升级。

use std::time::Duration;

use uuid::Uuid;

use crate::store::Db;
use crate::store::alert_repo::AlertRepo;
use crate::store::status_event_repo::StatusEventRepo;

pub const SWEEP_INTERVAL_SECS: u64 = 60;

/// 驱动一轮离线升级：每主机最新事件为未升级 offline 且超龄（created_at < now - alert_min）
/// 时写 alerts（metric_name="host_offline"，value=已离线分钟）并标记 escalated。
/// 返回本轮升级条数。alert_min == 0 表示禁用（直接返回 0）。
pub async fn sweep_once(db: &Db, alert_min: u64) -> anyhow::Result<u64> {
    if alert_min == 0 {
        return Ok(0);
    }
    let repo = StatusEventRepo::new(db.clone());
    let cutoff = chrono::Utc::now() - chrono::Duration::minutes(alert_min as i64);
    let mut escalated = 0u64;
    for row in repo.latest_per_host().await? {
        if row.event != "offline" || row.escalated || row.created_at >= cutoff {
            continue;
        }
        let host_uuid = match Uuid::parse_str(&row.host_id) {
            Ok(u) => u,
            Err(_) => {
                tracing::warn!(host_id = %row.host_id, "offline escalate: host_id 非 uuid，跳过升级");
                continue;
            }
        };
        let elapsed_min = (chrono::Utc::now() - row.created_at).num_minutes().max(1);
        let inserted = AlertRepo::new(db.clone())
            .insert(
                host_uuid,
                "host_offline",
                alert_min as f64,
                elapsed_min as f64,
            )
            .await;
        match inserted {
            Ok(_) => {
                repo.mark_escalated(row.id).await?;
                escalated += 1;
                tracing::warn!(host_id = %row.host_id, minutes = elapsed_min, "offline escalated to alert");
            }
            Err(e) => {
                // 主机可能已注销（alerts.host_id FK 失败）：同样标记已处置，避免每轮重试
                tracing::warn!(host_id = %row.host_id, error = %e, "offline escalate: alert insert failed，标记已处置");
                repo.mark_escalated(row.id).await?;
            }
        }
    }
    Ok(escalated)
}

/// 周期驱动（60s 一轮）。alert_min == 0 时不启动。
pub fn spawn_offline_alert_sweeper(db: Db, alert_min: u64) {
    if alert_min == 0 {
        tracing::info!("offline alert sweeper disabled (HELM_OFFLINE_ALERT_MINS=0)");
        return;
    }
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(SWEEP_INTERVAL_SECS)).await;
            match sweep_once(&db, alert_min).await {
                Ok(n) if n > 0 => {
                    tracing::info!(escalated = n, "offline alert sweep escalated");
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(error = %e, "offline alert sweep failed"),
            }
        }
    });
}
