//! Alert 仓储：alerts 表的读写。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `alerts` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct AlertRow {
    pub id: Uuid,
    pub host_id: Uuid,
    pub metric_name: String,
    pub threshold: f64,
    pub value: f64,
    pub level: String,
    pub created_at: DateTime<Utc>,
}

/// alerts 表仓储。
#[derive(Clone)]
pub struct AlertRepo {
    db: Db,
}

impl AlertRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 写入一条告警。
    pub async fn insert(
        &self,
        host_id: Uuid,
        metric_name: &str,
        threshold: f64,
        value: f64,
    ) -> sqlx::Result<AlertRow> {
        sqlx::query_as::<_, AlertRow>(
            "INSERT INTO alerts (host_id, metric_name, threshold, value)
             VALUES ($1, $2, $3, $4)
             RETURNING id, host_id, metric_name, threshold, value, level, created_at",
        )
        .bind(host_id)
        .bind(metric_name)
        .bind(threshold)
        .bind(value)
        .fetch_one(self.db.pool())
        .await
    }

    /// 列出最近的告警（按时间倒序）。
    pub async fn list(&self, limit: i64) -> sqlx::Result<Vec<AlertRow>> {
        sqlx::query_as::<_, AlertRow>(
            "SELECT id, host_id, metric_name, threshold, value, level, created_at
             FROM alerts ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
    }

    /// 删除指定时间之前的告警（时序保留）。
    pub async fn delete_before(&self, cutoff: DateTime<Utc>) -> sqlx::Result<u64> {
        sqlx::query("DELETE FROM alerts WHERE created_at < $1")
            .bind(cutoff)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected())
    }
}
