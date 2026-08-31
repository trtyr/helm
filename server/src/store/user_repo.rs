//! User 仓储：users 表的读写。

use crate::store::Db;
use sqlx::FromRow;
use uuid::Uuid;

/// `users` 表行。
#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub role: String,
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
            "SELECT id, username, password_hash, role
             FROM users WHERE username = $1 AND deleted_at IS NULL",
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
             RETURNING id, username, password_hash, role",
        )
        .bind(username)
        .bind(password_hash)
        .bind(role)
        .fetch_one(self.db.pool())
        .await
    }
}
