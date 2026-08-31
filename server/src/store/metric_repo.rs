//! Metric 仓储：metrics 表查询。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `metrics` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct MetricRow {
    pub host_id: Uuid,
    pub name: String,
    pub value: f64,
    pub ts: DateTime<Utc>,
}

/// metrics 表仓储。
#[derive(Clone)]
pub struct MetricRepo {
    db: Db,
}

impl MetricRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 查询某主机最近的指标（按时间倒序，限制条数）。
    pub async fn recent(&self, host_id: Uuid, limit: i64) -> sqlx::Result<Vec<MetricRow>> {
        sqlx::query_as::<_, MetricRow>(
            "SELECT host_id, name, value, ts
             FROM metrics WHERE host_id = $1 ORDER BY ts DESC LIMIT $2",
        )
        .bind(host_id)
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
    }

    /// 插入一条指标。
    pub async fn insert(
        &self,
        host_id: Uuid,
        name: &str,
        value: f64,
        ts: DateTime<Utc>,
    ) -> sqlx::Result<()> {
        sqlx::query("INSERT INTO metrics (host_id, name, value, ts) VALUES ($1, $2, $3, $4)")
            .bind(host_id)
            .bind(name)
            .bind(value)
            .bind(ts)
            .execute(self.db.pool())
            .await?;
        Ok(())
    }
}
