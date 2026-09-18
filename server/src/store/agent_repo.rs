//! Agent 仓储：注册时落库 host + agent（事务）。

use crate::store::Db;
use sqlx::FromRow;
use uuid::Uuid;

/// `agents` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct AgentRow {
    pub id: String,
    pub host_id: Uuid,
    pub version: String,
    pub registered_at: chrono::DateTime<chrono::Utc>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub elevated: bool,
}

/// 兜底下线扫描用的行：agent + host 定位。
#[derive(Debug, Clone, FromRow)]
pub struct StaleAgentRow {
    pub id: String,
    pub host_id: Uuid,
    pub hostname: String,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// 注册上报的主机系统细节（proto HostInfo 扩展字段；旧版 agent 不上报时用 Default）。
#[derive(Debug, Clone, Copy, Default)]
pub struct HostOsDetails<'a> {
    pub os_version: &'a str,
    pub kernel: &'a str,
    pub uptime_secs: u64,
}

/// Agent 注册仓储。
#[derive(Clone)]
pub struct AgentRepo {
    db: Db,
}

impl AgentRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 心跳落在 `[since, until)` 区间的 agent（含 hostname，兜底下线扫描用）。
    pub async fn list_stale_since(
        &self,
        since: chrono::DateTime<chrono::Utc>,
        until: chrono::DateTime<chrono::Utc>,
    ) -> sqlx::Result<Vec<StaleAgentRow>> {
        sqlx::query_as::<_, StaleAgentRow>(
            "SELECT a.id, a.host_id, h.hostname, a.last_heartbeat_at
             FROM agents a JOIN hosts h ON h.id = a.host_id
             WHERE a.last_heartbeat_at IS NOT NULL
               AND a.last_heartbeat_at >= $1
               AND a.last_heartbeat_at < $2",
        )
        .bind(since)
        .bind(until)
        .fetch_all(self.db.pool())
        .await
    }

    /// 注册（或更新）Agent，并关联（复用或新建）host。返回 host_id。
    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::too_many_arguments)]
    pub async fn register(
        &self,
        agent_id: &str,
        version: &str,
        hostname: &str,
        os: &str,
        arch: &str,
        platform: &str,
        public_ip: &str,
        local_ips: &[String],
        elevated: bool,
        details: HostOsDetails<'_>,
    ) -> sqlx::Result<Uuid> {
        let mut tx = self.db.pool().begin().await?;

        // 1. 按 hostname 复用或新建 host
        let host_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM hosts WHERE hostname = $1 AND deleted_at IS NULL LIMIT 1",
        )
        .bind(hostname)
        .fetch_optional(&mut *tx)
        .await?;

        let host_id = match host_id {
            Some(id) => {
                sqlx::query(
                    "UPDATE hosts SET os = $2, arch = $3, platform = $4,
                         public_ip = $5, local_ips = $6, os_version = $7, kernel = $8,
                         boot_at = CASE WHEN $9 > 0 THEN now() - ($9 * interval '1 second') ELSE boot_at END,
                         updated_at = now()
                     WHERE id = $1",
                )
                .bind(id)
                .bind(os)
                .bind(arch)
                .bind(platform)
                .bind(public_ip)
                .bind(local_ips)
                .bind(details.os_version)
                .bind(details.kernel)
                .bind(details.uptime_secs as i64)
                .execute(&mut *tx)
                .await?;
                id
            }
            None => sqlx::query_scalar(
                "INSERT INTO hosts (hostname, os, arch, platform, conn_mode, public_ip, local_ips,
                                        os_version, kernel, boot_at)
                     VALUES ($1, $2, $3, $4, 'reverse', $5, $6, $7, $8,
                             CASE WHEN $9 > 0 THEN now() - ($9 * interval '1 second') END)
                     RETURNING id",
            )
            .bind(hostname)
            .bind(os)
            .bind(arch)
            .bind(platform)
            .bind(public_ip)
            .bind(local_ips)
            .bind(details.os_version)
            .bind(details.kernel)
            .bind(details.uptime_secs as i64)
            .fetch_one(&mut *tx)
            .await?,
        };

        // 2. upsert agent
        sqlx::query(
            "INSERT INTO agents (id, host_id, version, registered_at, last_heartbeat_at, elevated)
             VALUES ($1, $2, $3, now(), now(), $4)
             ON CONFLICT (id) DO UPDATE SET host_id = $2, version = $3,
                 registered_at = now(), last_heartbeat_at = now(), elevated = $4",
        )
        .bind(agent_id)
        .bind(host_id)
        .bind(version)
        .bind(elevated)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(host_id)
    }

    /// forward 模式注册：agent 挂到既定 host 下（不按 hostname 查找/新建 host）。
    #[allow(clippy::too_many_arguments)]
    pub async fn register_under_host(
        &self,
        host_id: Uuid,
        agent_id: &str,
        version: &str,
        os: &str,
        arch: &str,
        platform: &str,
        public_ip: &str,
        local_ips: &[String],
        elevated: bool,
        details: HostOsDetails<'_>,
    ) -> sqlx::Result<()> {
        let mut tx = self.db.pool().begin().await?;

        // 更新 host 系统信息（保留 hostname/conn_mode/addr/tags 等声明字段）
        sqlx::query(
            "UPDATE hosts SET os = $2, arch = $3, platform = $4,
                 public_ip = $5, local_ips = $6, os_version = $7, kernel = $8,
                 boot_at = CASE WHEN $9 > 0 THEN now() - ($9 * interval '1 second') ELSE boot_at END,
                 updated_at = now()
             WHERE id = $1",
        )
        .bind(host_id)
        .bind(os)
        .bind(arch)
        .bind(platform)
        .bind(public_ip)
        .bind(local_ips)
        .bind(details.os_version)
        .bind(details.kernel)
        .bind(details.uptime_secs as i64)
        .execute(&mut *tx)
        .await?;

        // upsert agent
        sqlx::query(
            "INSERT INTO agents (id, host_id, version, registered_at, last_heartbeat_at, elevated)
             VALUES ($1, $2, $3, now(), now(), $4)
             ON CONFLICT (id) DO UPDATE SET host_id = $2, version = $3,
                 registered_at = now(), last_heartbeat_at = now(), elevated = $4",
        )
        .bind(agent_id)
        .bind(host_id)
        .bind(version)
        .bind(elevated)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    /// 按 agent_id 查 host_id。
    pub async fn get_host_id(&self, agent_id: &str) -> sqlx::Result<Option<Uuid>> {
        sqlx::query_scalar("SELECT host_id FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(self.db.pool())
            .await
    }

    /// 该 host 最近注册的 agent id（cancel 等按 host 找 agent 的场景）。
    pub async fn find_latest_agent_id(&self, host_id: Uuid) -> sqlx::Result<Option<String>> {
        sqlx::query_scalar(
            "SELECT id FROM agents WHERE host_id = $1 ORDER BY registered_at DESC LIMIT 1",
        )
        .bind(host_id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 更新心跳时间。
    pub async fn update_heartbeat(&self, agent_id: &str, ts_ms: u64) -> sqlx::Result<()> {
        let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(ts_ms as i64)
            .unwrap_or_else(chrono::Utc::now);
        sqlx::query("UPDATE agents SET last_heartbeat_at = $2 WHERE id = $1")
            .bind(agent_id)
            .bind(ts)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// 按 host 列出所有 agent_id（在线判定用）。
    pub async fn list_agent_ids(&self, host_id: Uuid) -> sqlx::Result<Vec<String>> {
        sqlx::query_scalar("SELECT id FROM agents WHERE host_id = $1")
            .bind(host_id)
            .fetch_all(self.db.pool())
            .await
    }

    /// 取 host 下最近注册的一个 agent（主机列表合并展示 Agent 标识/版本用）。
    pub async fn first_by_host(&self, host_id: Uuid) -> sqlx::Result<Option<AgentRow>> {
        sqlx::query_as::<_, AgentRow>(
            "SELECT id, host_id, version, registered_at, last_heartbeat_at, elevated
             FROM agents WHERE host_id = $1 ORDER BY registered_at DESC LIMIT 1",
        )
        .bind(host_id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 主机最近心跳时间（取该 host 所有 agent 的最大 last_heartbeat_at）。
    pub async fn last_heartbeat(
        &self,
        host_id: Uuid,
    ) -> sqlx::Result<Option<chrono::DateTime<chrono::Utc>>> {
        sqlx::query_scalar("SELECT MAX(last_heartbeat_at) FROM agents WHERE host_id = $1")
            .bind(host_id)
            .fetch_one(self.db.pool())
            .await
    }

    /// 列出所有 agent（按注册时间倒序）。
    pub async fn list_all(&self) -> sqlx::Result<Vec<AgentRow>> {
        sqlx::query_as::<_, AgentRow>(
            "SELECT id, host_id, version, registered_at, last_heartbeat_at, elevated
             FROM agents ORDER BY registered_at DESC",
        )
        .fetch_all(self.db.pool())
        .await
    }

    /// 按 agent_id 查行。
    pub async fn get(&self, agent_id: &str) -> sqlx::Result<Option<AgentRow>> {
        sqlx::query_as::<_, AgentRow>(
            "SELECT id, host_id, version, registered_at, last_heartbeat_at, elevated
             FROM agents WHERE id = $1",
        )
        .bind(agent_id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 注销：永久删除 agent 记录。
    pub async fn delete(&self, agent_id: &str) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM agents WHERE id = $1")
            .bind(agent_id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}

impl AgentRepo {
    /// 掉线期间挂起的下线/注销操作。
    pub async fn mark_pending_offline(&self, agent_id: &str, action: &str) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO agent_pending_offline (agent_id, action) VALUES ($1, $2)
         ON CONFLICT (agent_id) DO UPDATE SET action = EXCLUDED.action, created_at = now()",
        )
        .bind(agent_id)
        .bind(action)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 取出并清除挂起操作（重连瞬间调用）。
    pub async fn take_pending_offline(&self, agent_id: &str) -> sqlx::Result<Option<String>> {
        let row: Option<(String,)> = sqlx::query_as(
            "DELETE FROM agent_pending_offline WHERE agent_id = $1 RETURNING action",
        )
        .bind(agent_id)
        .fetch_optional(self.db.pool())
        .await?;
        Ok(row.map(|r| r.0))
    }

    /// TTL 清理（F1）：删除超过 `ttl` 未被消费的挂起操作，返回删除的 agent 数。
    /// 挂起操作长期无人认领 = agent 长期未重连，过期作废并告警（操作者可见）。
    pub async fn expire_stale_pending_offline(
        &self,
        ttl: chrono::Duration,
    ) -> sqlx::Result<Vec<(String, String)>> {
        sqlx::query_as(
            "DELETE FROM agent_pending_offline
             WHERE created_at < now() - make_interval(secs => $1)
             RETURNING agent_id, action",
        )
        .bind(ttl.num_seconds() as f64)
        .fetch_all(self.db.pool())
        .await
    }
}
