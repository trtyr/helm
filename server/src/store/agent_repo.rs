//! Agent 仓储：注册时落库 host + agent（事务）。

use crate::store::Db;
use uuid::Uuid;

/// Agent 注册仓储。
#[derive(Clone)]
pub struct AgentRepo {
    db: Db,
}

impl AgentRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 注册（或更新）Agent，并关联（复用或新建）host。返回 host_id。
    #[allow(clippy::too_many_arguments)]
    pub async fn register(
        &self,
        agent_id: &str,
        version: &str,
        hostname: &str,
        os: &str,
        arch: &str,
        platform: &str,
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
                    "UPDATE hosts SET os = $2, arch = $3, platform = $4, updated_at = now() WHERE id = $1",
                )
                .bind(id)
                .bind(os)
                .bind(arch)
                .bind(platform)
                .execute(&mut *tx)
                .await?;
                id
            }
            None => {
                sqlx::query_scalar(
                    "INSERT INTO hosts (hostname, os, arch, platform, conn_mode)
                     VALUES ($1, $2, $3, $4, 'reverse') RETURNING id",
                )
                .bind(hostname)
                .bind(os)
                .bind(arch)
                .bind(platform)
                .fetch_one(&mut *tx)
                .await?
            }
        };

        // 2. upsert agent
        sqlx::query(
            "INSERT INTO agents (id, host_id, version, registered_at, last_heartbeat_at)
             VALUES ($1, $2, $3, now(), now())
             ON CONFLICT (id) DO UPDATE SET host_id = $2, version = $3,
                 registered_at = now(), last_heartbeat_at = now()",
        )
        .bind(agent_id)
        .bind(host_id)
        .bind(version)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(host_id)
    }

    /// 按 agent_id 查 host_id。
    pub async fn get_host_id(&self, agent_id: &str) -> sqlx::Result<Option<Uuid>> {
        sqlx::query_scalar("SELECT host_id FROM agents WHERE id = $1")
            .bind(agent_id)
            .fetch_optional(self.db.pool())
            .await
    }
}
