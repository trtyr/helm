//! 服务仓储：services 表的读写。

use crate::store::Db;
use sqlx::FromRow;
use uuid::Uuid;

/// `services` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct ServiceRow {
    pub id: Uuid,
    pub host_id: Uuid,
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: String,
    pub restart_policy: String,
    pub pid: Option<i32>,
    pub exit_code: Option<i32>,
    pub log: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// services 表仓储。
#[derive(Clone)]
pub struct ServiceRepo {
    db: Db,
}

impl ServiceRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 创建服务并返回完整行。
    pub async fn create(
        &self,
        host_id: Uuid,
        name: &str,
        command: &str,
        args: &[String],
        restart_policy: &str,
    ) -> sqlx::Result<ServiceRow> {
        sqlx::query_as::<_, ServiceRow>(
            "INSERT INTO services (host_id, name, command, args, restart_policy)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, host_id, name, command, args, status, restart_policy, pid, exit_code, log, created_at, updated_at",
        )
        .bind(host_id)
        .bind(name)
        .bind(command)
        .bind(args)
        .bind(restart_policy)
        .fetch_one(self.db.pool())
        .await
    }

    /// 列出所有服务（按创建时间倒序）。
    pub async fn list(&self) -> sqlx::Result<Vec<ServiceRow>> {
        sqlx::query_as::<_, ServiceRow>(
            "SELECT id, host_id, name, command, args, status, restart_policy, pid, exit_code, log, created_at, updated_at
             FROM services ORDER BY created_at DESC",
        )
        .fetch_all(self.db.pool())
        .await
    }

    /// 分页列出服务。
    pub async fn list_paged(&self, limit: i64, offset: i64) -> sqlx::Result<Vec<ServiceRow>> {
        sqlx::query_as::<_, ServiceRow>(
            "SELECT id, host_id, name, command, args, status, restart_policy, pid, exit_code, log, created_at, updated_at
             FROM services ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
    }

    /// 按 host 过滤列出（D4 前端接入：详情页只看本机常驻服务）。
    pub async fn list_by_host(&self, host_id: Uuid, limit: i64) -> sqlx::Result<Vec<ServiceRow>> {
        sqlx::query_as::<_, ServiceRow>(
            "SELECT id, host_id, name, command, args, status, restart_policy, pid, exit_code, log, created_at, updated_at
             FROM services WHERE host_id = $1 ORDER BY created_at DESC LIMIT $2",
        )
        .bind(host_id)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
    }

    /// 按 id 查服务。
    pub async fn get(&self, id: Uuid) -> sqlx::Result<Option<ServiceRow>> {
        sqlx::query_as::<_, ServiceRow>(
            "SELECT id, host_id, name, command, args, status, restart_policy, pid, exit_code, log, created_at, updated_at
             FROM services WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 更新状态 + pid + exit_code。
    pub async fn set_status(
        &self,
        id: Uuid,
        status: &str,
        pid: Option<i32>,
        exit_code: Option<i32>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE services SET status = $2, pid = $3, exit_code = $4, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(pid)
        .bind(exit_code)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 追加日志（保留最近 64KB，滚动截断）。
    pub async fn append_log(&self, id: Uuid, log: &[u8]) -> sqlx::Result<()> {
        let text = String::from_utf8_lossy(log).to_string();
        sqlx::query(
            "UPDATE services SET log = right(log || $2, 65536), updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(text)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 删除服务（停掉的才删，运行中返回 None 由调用方判断）。
    pub async fn delete(&self, id: Uuid) -> sqlx::Result<u64> {
        sqlx::query("DELETE FROM services WHERE id = $1 AND status <> 'running'")
            .bind(id)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected())
    }

    /// 更新服务（name/command/args/restart_policy），返回更新后的行。
    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        command: &str,
        args: &[String],
        restart_policy: &str,
    ) -> sqlx::Result<Option<ServiceRow>> {
        sqlx::query_as::<_, ServiceRow>(
            "UPDATE services SET name = $2, command = $3, args = $4, restart_policy = $5, updated_at = now()
             WHERE id = $1
             RETURNING id, host_id, name, command, args, status, restart_policy, pid, exit_code, log, created_at, updated_at",
        )
        .bind(id)
        .bind(name)
        .bind(command)
        .bind(args)
        .bind(restart_policy)
        .fetch_optional(self.db.pool())
        .await
    }
}
