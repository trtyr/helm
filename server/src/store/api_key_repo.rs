//! API key 仓储：api_keys 表的读写（机器对机器认证，决策 010）。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `api_keys` 表行。`key_hash` 仅内部使用，不对外序列化（HTTP 层用视图）。
#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRow {
    pub id: Uuid,
    pub name: String,
    pub key_hash: String,
    pub prefix: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    /// 功能域 scope；空 = 全功能（兼容 0016 之前的存量 key）。
    pub scopes: Vec<String>,
}

/// api_keys 表仓储。
#[derive(Clone)]
pub struct ApiKeyRepo {
    db: Db,
}

impl ApiKeyRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 插入一把 key（哈希与展示前缀由应用层生成）。
    pub async fn insert(
        &self,
        name: &str,
        key_hash: &str,
        prefix: &str,
        expires_at: Option<DateTime<Utc>>,
        scopes: &[String],
    ) -> sqlx::Result<ApiKeyRow> {
        sqlx::query_as::<_, ApiKeyRow>(
            "INSERT INTO api_keys (name, key_hash, prefix, expires_at, scopes)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, name, key_hash, prefix, created_at, last_used_at, expires_at, revoked_at, scopes",
        )
        .bind(name)
        .bind(key_hash)
        .bind(prefix)
        .bind(expires_at)
        .bind(scopes)
        .fetch_one(self.db.pool())
        .await
    }

    /// 按哈希查有效 key：未吊销且未过期才算命中。
    pub async fn find_valid_by_hash(&self, key_hash: &str) -> sqlx::Result<Option<ApiKeyRow>> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, name, key_hash, prefix, created_at, last_used_at, expires_at, revoked_at, scopes
             FROM api_keys
             WHERE key_hash = $1
               AND revoked_at IS NULL
               AND (expires_at IS NULL OR expires_at > now())",
        )
        .bind(key_hash)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 按 id 查（含已吊销，供管理端点展示）。
    pub async fn get(&self, id: Uuid) -> sqlx::Result<Option<ApiKeyRow>> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, name, key_hash, prefix, created_at, last_used_at, expires_at, revoked_at, scopes
             FROM api_keys WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 分页列出（按创建时间倒序）。
    pub async fn list_paged(&self, limit: i64, offset: i64) -> sqlx::Result<Vec<ApiKeyRow>> {
        sqlx::query_as::<_, ApiKeyRow>(
            "SELECT id, name, key_hash, prefix, created_at, last_used_at, expires_at, revoked_at, scopes
             FROM api_keys
             ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
    }

    /// 刷新最后使用时间（认证命中时调用，尽力而为）。
    pub async fn touch_last_used(&self, id: Uuid) -> sqlx::Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = now() WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await
            .map(|_| ())
    }

    /// 吊销。返回是否命中（幂等：已吊销返回 false）。
    pub async fn revoke(&self, id: Uuid) -> sqlx::Result<bool> {
        sqlx::query("UPDATE api_keys SET revoked_at = now() WHERE id = $1 AND revoked_at IS NULL")
            .bind(id)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected() > 0)
    }

    /// 删除指定时间之前且已吊销的 key（时序保留，与 metrics/alerts 一致由后台任务调用）。
    pub async fn delete_revoked_before(&self, cutoff: DateTime<Utc>) -> sqlx::Result<u64> {
        sqlx::query("DELETE FROM api_keys WHERE revoked_at IS NOT NULL AND revoked_at < $1")
            .bind(cutoff)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected())
    }
}
