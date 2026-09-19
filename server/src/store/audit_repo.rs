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

    /// 分页列出审计记录。sort（P001-T1c）：白名单字段排序，未命中回退 created_at DESC。
    /// q（P001-T1）：actor/resource/detail 模糊搜索；action：动作精确过滤。
    pub async fn list_paged(
        &self,
        limit: i64,
        offset: i64,
        sort: Option<(String, bool)>,
        q: Option<&str>,
        action: Option<&str>,
    ) -> sqlx::Result<Vec<AuditRow>> {
        let (field, desc) = sort
            .as_ref()
            .map(|(f, d)| (f.as_str(), *d))
            .unwrap_or(("created_at", true));
        let order = super::order_by(
            field,
            desc,
            &[
                ("created_at", "created_at {dir}"),
                ("actor", "actor {dir}"),
                ("action", "action {dir}"),
                ("resource", "resource {dir}"),
            ],
            "created_at DESC",
        );
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, actor, action, resource, detail, created_at FROM audit_logs",
        );
        if let Some(a) = action {
            qb.push(" WHERE action = ").push_bind(a.to_string());
        }
        if let Some(kw) = q {
            let pat = format!("%{kw}%");
            qb.push(if action.is_some() {
                " AND ("
            } else {
                " WHERE ("
            })
            .push("actor ILIKE ")
            .push_bind(pat.clone())
            .push(" OR resource ILIKE ")
            .push_bind(pat.clone())
            .push(" OR detail ILIKE ")
            .push_bind(pat)
            .push(")");
        }
        qb.push(" ORDER BY ")
            .push(order)
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        qb.build_query_as::<AuditRow>()
            .fetch_all(self.db.pool())
            .await
    }

    /// retention（C1）：删除 `cutoff` 之前的审计记录，返回删除行数。
    pub async fn delete_before(&self, cutoff: DateTime<Utc>) -> sqlx::Result<u64> {
        let result = sqlx::query("DELETE FROM audit_logs WHERE created_at < $1")
            .bind(cutoff)
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }
}
