//! 应用层：通知中心（决策 009——系统内部小卡片：上线 / 下线 / 预警，不外发）。
//!
//! 冷却窗口：同 host 同类型通知在 [`COOLDOWN_SECS`] 内合并为一条
//! （刷新消息与时间、重置未读），压平网络抖动 / Server 重启风暴的刷屏。

use crate::domain::Result;
use crate::grpc::stream_registry::StreamRegistry;
use crate::store::Db;
use crate::store::agent_repo::{AgentRepo, StaleAgentRow};
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

/// 上线落库 + 通知的**唯一实现**（reverse/forward 同构）。
///
/// 审计缺陷修复（2026-09-20）：此前 reverse（agent_service）落状态事件、forward（forward_manager）
/// 只发通知不落库，导致 forward 主机的 /logs/events 时间线永远缺上线事件，离线时长列（LEAD 窗口回填）
/// 对它们永远显示「进行中」。抽成共享函数后两种注册模式不可能再分叉——任何新增注册路径都必须调本函数。
pub async fn record_online_and_notify(
    db: &Db,
    streams: &StreamRegistry,
    host_id: Uuid,
    hostname: &str,
) {
    // 同点同语义：先落状态事件，再发通知
    if let Err(e) = crate::store::status_event_repo::StatusEventRepo::new(db.clone())
        .insert(&host_id.to_string(), "online", "registered", "")
        .await
    {
        tracing::warn!(host_id = %host_id, error = ?e, "online status event insert failed");
    }
    let svc = NotificationService::new(db.clone(), streams.clone());
    if let Err(e) = svc
        .notify(host_id, KIND_ONLINE, &format!("主机 {hostname} 已上线"))
        .await
    {
        tracing::warn!(host_id = %host_id, error = ?e, "online notify failed");
    }
}

/// 状态事件落库的唯一入口（G3 收口 2026-09-21：grpc 层不再直构 `StatusEventRepo`）。
///
/// 失败只告警不返回错误：状态事件是时间线的补充信息，丢一条不影响业务正确性，
/// 但会削弱事后追溯，因此必须可见（T3 可观测契约），不静默丢弃。
pub async fn record_status_event(db: &Db, host_id: Uuid, event: &str, reason: &str, detail: &str) {
    if let Err(e) = crate::store::status_event_repo::StatusEventRepo::new(db.clone())
        .insert(&host_id.to_string(), event, reason, detail)
        .await
    {
        tracing::warn!(host_id = %host_id, event, error = ?e, "status event insert failed");
    }
}

/// 半开死连接的单行处置：① 注册表仍在线则注销；② 该通知时先落 offline 状态事件再发通知。
///
/// 审计缺陷修复（2026-09-20）：静默掉线（心跳超时）是离线升级告警最该覆盖的场景，但此前本路径只发通知
/// 不落状态事件；且注销发生在 h2 keepalive 关闭流之前，随后 `on_disconnect` 会被 `unregister_if_current`
/// 守卫挡掉（不重复落库）。两者叠加导致「静默死亡」的主机最新事件停在 online，
/// `offline_alert_sweeper` 的 `row.event != "offline"` 判断永远跳过——升级告警对最该告警的场景失效。
pub async fn handle_stale_row(
    db: &Db,
    connections: &crate::grpc::connection_registry::ConnectionRegistry,
    svc: &NotificationService,
    row: &StaleAgentRow,
    now: DateTime<Utc>,
    timeout_secs: u64,
) {
    // 半开死连接：注册表仍在线则注销
    if connections.is_online(&row.id).await {
        tracing::info!(agent_id = %row.id, "sweeper: unregister stale half-open connection");
        connections.unregister(&row.id).await;
    }
    let Some(last_hb) = row.last_heartbeat_at else {
        return;
    };
    if !should_notify_stale(last_hb, now, timeout_secs as i64) {
        return;
    }
    // 同点同语义：先落 offline 状态事件（reason=heartbeat_timeout），再发通知
    if let Err(e) = crate::store::status_event_repo::StatusEventRepo::new(db.clone())
        .insert(&row.host_id.to_string(), "offline", "heartbeat_timeout", "")
        .await
    {
        tracing::warn!(agent_id = %row.id, error = ?e, "stale offline status event insert failed");
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
        q: Option<&str>,
    ) -> Result<Vec<NotificationRow>> {
        Ok(NotificationRepo::new(self.db.clone())
            .list_paged(limit, offset, unread_only, sort, q)
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
                handle_stale_row(&db, &connections, &svc, &row, now, timeout_secs).await;
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
