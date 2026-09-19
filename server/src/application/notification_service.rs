//! 应用层：通知中心（决策 009——系统内部小卡片：上线 / 下线 / 预警，不外发）。
//!
//! 冷却窗口：同 host 同类型通知在 [`COOLDOWN_SECS`] 内合并为一条
//! （刷新消息与时间、重置未读），压平网络抖动 / Server 重启风暴的刷屏。

use crate::domain::Result;
use crate::grpc::stream_registry::StreamRegistry;
use crate::store::Db;
use crate::store::agent_repo::AgentRepo;
use crate::store::notification_repo::{NotificationRepo, NotificationRow};
use chrono::{DateTime, Utc};
use uuid::Uuid;

/// 冷却窗口（秒）：同 host 同类型通知在该窗口内只保留最新一条。
pub const COOLDOWN_SECS: i64 = 300;

/// 通知类型常量（与迁移 0008 的 CHECK 约束一致）。
pub const KIND_ONLINE: &str = "online";
pub const KIND_OFFLINE: &str = "offline";
pub const KIND_ALERT: &str = "alert";

/// 判定新通知是否落在同类型上一条的冷却窗口内（纯函数，便于测试）。
///
/// `now - last < COOLDOWN_SECS` 即合并；时钟回拨（负差值）保守视为窗口内。
pub fn within_cooldown(last_created_at: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    now.signed_duration_since(last_created_at).num_seconds() < COOLDOWN_SECS
}

/// 兜底扫描判定（纯函数，便于测试）：心跳已超时（stale）但**刚进入** stale
/// 窗口（`now - 2*timeout <= last_heartbeat < now - timeout`）才补发下线通知；
/// 掉线已久的不再重复通知（首次掉线时断连即发路径或上一轮扫描已发过）。
pub fn should_notify_stale(
    last_heartbeat: DateTime<Utc>,
    now: DateTime<Utc>,
    timeout_secs: i64,
) -> bool {
    let age = now.signed_duration_since(last_heartbeat).num_seconds();
    age >= timeout_secs && age < timeout_secs * 2
}

/// 通知中心用例：记录通知（冷却合并 + 落库 + 实时广播）与查询。
#[derive(Clone)]
pub struct NotificationService {
    db: Db,
    streams: StreamRegistry,
}

impl NotificationService {
    pub fn new(db: Db, streams: StreamRegistry) -> Self {
        Self { db, streams }
    }

    /// 记录一条通知：冷却窗口内合并刷新，否则插入新行；随后实时广播。
    pub async fn notify(
        &self,
        host_id: Uuid,
        kind: &str,
        message: &str,
    ) -> Result<NotificationRow> {
        let repo = NotificationRepo::new(self.db.clone());
        let now = Utc::now();
        let row = match repo.latest_of_type(host_id, kind).await? {
            Some(last) if within_cooldown(last.created_at, now) => {
                repo.refresh(last.id, message).await?
            }
            _ => repo.insert(host_id, kind, message).await?,
        };
        let payload = serde_json::json!({
            "id": row.id,
            "host_id": row.host_id,
            "kind": row.kind,
            "message": row.message,
            "read": row.read,
            "created_at": row.created_at.to_rfc3339(),
        });
        self.streams
            .broadcast("notifications", payload.to_string().into_bytes())
            .await;
        Ok(row)
    }

    /// 分页列出通知。
    pub async fn list_paged(
        &self,
        limit: i64,
        offset: i64,
        unread_only: bool,
        sort: Option<(String, bool)>,
    ) -> Result<Vec<NotificationRow>> {
        Ok(NotificationRepo::new(self.db.clone())
            .list_paged(limit, offset, unread_only, sort)
            .await?)
    }

    /// 未读数。
    pub async fn unread_count(&self) -> Result<i64> {
        Ok(NotificationRepo::new(self.db.clone()).count(true).await?)
    }

    /// 标记单条已读。返回是否命中。
    pub async fn mark_read(&self, id: Uuid) -> Result<bool> {
        Ok(NotificationRepo::new(self.db.clone()).mark_read(id).await?)
    }

    /// 全部标记已读。返回影响行数。
    pub async fn mark_all_read(&self) -> Result<u64> {
        Ok(NotificationRepo::new(self.db.clone())
            .mark_all_read()
            .await?)
    }
}

/// 兜底下线扫描：周期与心跳超时阈值同量级，后台运行不阻塞主链路。
///
/// 覆盖断连即发（`on_disconnect`）漏掉的场景：半开连接（对端无声消失，流未断）。
/// 对 stale 的 agent：若注册表仍标记在线（半开死连接），先注销再通知；
/// 只有「刚进入 stale」的才补发通知（掉线已久不重复打扰），冷却窗口兜底合并。
pub fn spawn_offline_sweeper(
    db: Db,
    registry: StreamRegistry,
    connections: crate::grpc::connection_registry::ConnectionRegistry,
    timeout_secs: u64,
) {
    tokio::spawn(async move {
        let timeout = std::time::Duration::from_secs(timeout_secs.max(1));
        loop {
            tokio::time::sleep(timeout).await;
            let now = Utc::now();
            let since = now - chrono::Duration::seconds(timeout_secs as i64 * 2);
            let until = now - chrono::Duration::seconds(timeout_secs as i64);
            let rows = match AgentRepo::new(db.clone())
                .list_stale_since(since, until)
                .await
            {
                Ok(rows) => rows,
                Err(e) => {
                    tracing::warn!(error = %e, "offline sweeper query failed");
                    continue;
                }
            };
            let svc = NotificationService::new(db.clone(), registry.clone());
            for row in rows {
                // 半开死连接：注册表仍在线则注销
                if connections.is_online(&row.id).await {
                    tracing::info!(agent_id = %row.id, "sweeper: unregister stale half-open connection");
                    connections.unregister(&row.id).await;
                }
                if let Some(last_hb) = row.last_heartbeat_at {
                    if !should_notify_stale(last_hb, now, timeout_secs as i64) {
                        continue;
                    }
                    if let Err(e) = svc
                        .notify(
                            row.host_id,
                            KIND_OFFLINE,
                            &format!("主机 {} 已下线（心跳超时）", row.hostname),
                        )
                        .await
                    {
                        tracing::warn!(agent_id = %row.id, error = ?e, "sweeper notify failed");
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ts(secs: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_800_000_000, 0).unwrap() + chrono::Duration::seconds(secs)
    }

    #[test]
    fn within_window_coalesces() {
        // 窗口内（299s < 300s）→ 合并
        assert!(within_cooldown(ts(0), ts(299)));
        // 窗口刚过（恰好 300s）→ 放行新通知
        assert!(!within_cooldown(ts(0), ts(300)));
        // 窗口外 → 放行
        assert!(!within_cooldown(ts(0), ts(301)));
        assert!(!within_cooldown(ts(0), ts(3600)));
    }

    #[test]
    fn clock_skew_treated_as_cooldown() {
        // 时钟回拨（now < last）保守合并，避免风暴场景误插新行
        assert!(within_cooldown(ts(100), ts(50)));
    }

    #[test]
    fn stale_sweeper_notifies_only_freshly_stale() {
        let t = 30; // 心跳超时阈值 30s
        let now = ts(3600);
        // 未超时（age 25s）→ 不通知
        assert!(!should_notify_stale(ts(3600 - 25), now, t));
        // 刚进入 stale（30s ≤ age < 60s）→ 补发
        assert!(should_notify_stale(ts(3600 - 30), now, t));
        assert!(should_notify_stale(ts(3600 - 59), now, t));
        // 掉线已久（≥ 2×timeout）→ 不再重复通知
        assert!(!should_notify_stale(ts(3600 - 60), now, t));
        assert!(!should_notify_stale(ts(0), now, t));
    }
}
