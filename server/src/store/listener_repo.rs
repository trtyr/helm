//! Listener 仓储：listeners 表的读写。

use crate::store::Db;
use sqlx::FromRow;
use uuid::Uuid;

/// `listeners` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ListenerRow {
    pub id: Uuid,
    pub name: String,
    pub addr: String,
    pub proto: String,
    pub auth: String,
    pub status: String,
}

/// listeners 表仓储。
#[derive(Clone)]
pub struct ListenerRepo {
    db: Db,
}

impl ListenerRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 新建监听器（status = stopped）。
    pub async fn create(
        &self,
        name: &str,
        addr: &str,
        proto: &str,
        auth: &str,
    ) -> sqlx::Result<ListenerRow> {
        sqlx::query_as::<_, ListenerRow>(
            "INSERT INTO listeners (name, addr, proto, auth) VALUES ($1, $2, $3, $4)
             RETURNING id, name, addr, proto, auth, status",
        )
        .bind(name)
        .bind(addr)
        .bind(proto)
        .bind(auth)
        .fetch_one(self.db.pool())
        .await
    }

    /// 列出所有监听器（按创建时间）。
    pub async fn list(&self) -> sqlx::Result<Vec<ListenerRow>> {
        sqlx::query_as::<_, ListenerRow>(
            "SELECT id, name, addr, proto, auth, status FROM listeners ORDER BY created_at",
        )
        .fetch_all(self.db.pool())
        .await
    }

    /// 按 id 查询。
    pub async fn get(&self, id: Uuid) -> sqlx::Result<Option<ListenerRow>> {
        sqlx::query_as::<_, ListenerRow>(
            "SELECT id, name, addr, proto, auth, status FROM listeners WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 更新状态。
    pub async fn set_status(&self, id: Uuid, status: &str) -> sqlx::Result<()> {
        sqlx::query("UPDATE listeners SET status = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(status)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// 查询所有 running 状态的监听器（重启恢复用）。
    pub async fn list_running(&self) -> sqlx::Result<Vec<ListenerRow>> {
        sqlx::query_as::<_, ListenerRow>(
            "SELECT id, name, addr, proto, auth, status FROM listeners WHERE status = 'running'",
        )
        .fetch_all(self.db.pool())
        .await
    }

    /// 监听器总数。
    pub async fn count(&self) -> sqlx::Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM listeners")
            .fetch_one(self.db.pool())
            .await
    }

    /// 删除监听器（清理用）。
    pub async fn delete(&self, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("DELETE FROM listeners WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }

    /// 更新监听器（name/addr/proto/auth），返回更新后的行。
    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        addr: &str,
        proto: &str,
        auth: &str,
    ) -> sqlx::Result<Option<ListenerRow>> {
        sqlx::query_as::<_, ListenerRow>(
            "UPDATE listeners SET name = $2, addr = $3, proto = $4, auth = $5, updated_at = now()
             WHERE id = $1
             RETURNING id, name, addr, proto, auth, status",
        )
        .bind(id)
        .bind(name)
        .bind(addr)
        .bind(proto)
        .bind(auth)
        .fetch_optional(self.db.pool())
        .await
    }
}
