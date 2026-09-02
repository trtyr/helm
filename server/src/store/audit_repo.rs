//! Audit 仓储：audit_logs 表的读写。

use crate::store::Db;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::FromRow;
use uuid::Uuid;

/// `audit_logs` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct AuditRow {
    pub id: Uuid,
    pub actor: String,
    pub action: String,
    pub resource: String,
    pub detail: Value,
    pub created_at: DateTime<Utc>,
}

/// audit_logs 表仓储。
#[derive(Clone)]
pub struct AuditRepo {
    db: Db,
}

impl AuditRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 写入一条审计记录。
    pub async fn insert(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        detail: &Value,
    ) -> sqlx::Result<AuditRow> {
        sqlx::query_as::<_, AuditRow>(
            "INSERT INTO audit_logs (actor, action, resource, detail)
             VALUES ($1, $2, $3, $4)
             RETURNING id, actor, action, resource, detail, created_at",
        )
        .bind(actor)
        .bind(action)
        .bind(resource)
        .bind(detail)
        .fetch_one(self.db.pool())
        .await
    }

    /// 列出最近的审计记录（按时间倒序）。
    pub async fn list(&self, limit: i64) -> sqlx::Result<Vec<AuditRow>> {
        sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor, action, resource, detail, created_at
             FROM audit_logs ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
    }

    /// 分页列出审计记录。
    pub async fn list_paged(&self, limit: i64, offset: i64) -> sqlx::Result<Vec<AuditRow>> {
        sqlx::query_as::<_, AuditRow>(
            "SELECT id, actor, action, resource, detail, created_at
             FROM audit_logs ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
    }
}
