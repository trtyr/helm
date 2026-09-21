//! User 仓储：users 表的读写。

use crate::store::Db;
use sqlx::FromRow;
use uuid::Uuid;

/// `users` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// A5：JWT 版本号——改密/改名自增，旧 token 立即失效
    pub token_version: i64,
}

/// users 表仓储。
#[derive(Clone)]
pub struct UserRepo {
    db: Db,
}

impl UserRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 按用户名查用户。
    pub async fn get_by_username(&self, username: &str) -> sqlx::Result<Option<UserRow>> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, username, password_hash, role, created_at, token_version
             FROM users WHERE username = $1 AND deleted_at IS NULL",
        )
        .bind(username)
        .fetch_optional(self.db.pool())
        .await
    }

    /// A5：读某用户的 JWT 版本号（用户不存在返回 None）。
    pub async fn token_version(&self, username: &str) -> sqlx::Result<Option<i64>> {
        sqlx::query_scalar(
            "SELECT token_version FROM users WHERE username = $1 AND deleted_at IS NULL",
        )
        .bind(username)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 未删除用户数。
    pub async fn count(&self) -> sqlx::Result<i64> {
        sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE deleted_at IS NULL")
            .fetch_one(self.db.pool())
            .await
    }

    /// 新建用户。
    pub async fn create(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> sqlx::Result<UserRow> {
        sqlx::query_as::<_, UserRow>(
            "INSERT INTO users (username, password_hash, role) VALUES ($1, $2, $3)
             RETURNING id, username, password_hash, role, created_at, token_version",
        )
        .bind(username)
        .bind(password_hash)
        .bind(role)
        .fetch_one(self.db.pool())
        .await
    }

    /// 按 id 更新密码哈希。返回是否命中。
    pub async fn update_password(&self, id: Uuid, password_hash: &str) -> sqlx::Result<bool> {
        sqlx::query("UPDATE users SET password_hash = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(password_hash)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected() > 0)
    }

    /// 按 id 更新用户名（username UNIQUE 冲突返回 23505，由 Service 层转义）。返回是否命中。
    pub async fn update_username(&self, id: Uuid, username: &str) -> sqlx::Result<bool> {
        sqlx::query("UPDATE users SET username = $2, updated_at = now() WHERE id = $1")
            .bind(id)
            .bind(username)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected() > 0)
    }

    /// A5：自增 token 版本号（改密/改名调用）——此后签发的 JWT 才有效，旧 token 立即失效。
    pub async fn bump_token_version(&self, id: Uuid) -> sqlx::Result<bool> {
        sqlx::query(
            "UPDATE users SET token_version = token_version + 1, updated_at = now() WHERE id = $1",
        )
        .bind(id)
        .execute(self.db.pool())
        .await
        .map(|r| r.rows_affected() > 0)
    }
}
