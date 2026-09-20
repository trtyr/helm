//! 状态事件仓储（P003 T1）：agent 上下线/断连事件落库与查询。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `status_events` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct StatusEventRow {
    pub id: Uuid,
    pub host_id: String,
    pub event: String,
    pub reason: String,
    pub detail: String,
    pub created_at: DateTime<Utc>,
}

/// 状态事件仓储。
#[derive(Clone)]
pub struct StatusEventRepo {
    db: Db,
}

impl StatusEventRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 写入一条状态事件（event: "online" | "offline"）。
    pub async fn insert(
        &self,
        host_id: &str,
        event: &str,
        reason: &str,
        detail: &str,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO status_events (host_id, event, reason, detail) VALUES ($1, $2, $3, $4)",
        )
        .bind(host_id)
        .bind(event)
        .bind(reason)
        .bind(detail)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 分页查询（P001-T4 模式：count_filtered 必须镜像本函数的全部 WHERE 分支）。
    #[allow(clippy::too_many_arguments)]
    pub async fn list_paged(
        &self,
        q: Option<&str>,
        host_id: Option<&str>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        sort: Option<(String, bool)>,
        limit: i64,
        offset: i64,
    ) -> sqlx::Result<Vec<StatusEventRow>> {
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, host_id, event, reason, detail, created_at FROM status_events WHERE 1=1",
        );
        push_filters(&mut qb, q, host_id, from, to);
        let order = super::order_by(
            sort.as_ref().map(|(f, _)| f.as_str()).unwrap_or(""),
            sort.as_ref().map(|(_, d)| *d).unwrap_or(false),
            &[("created_at", "created_at {dir}")],
            "created_at DESC",
        );
        qb.push(" ORDER BY ").push(order);
        qb.push(" LIMIT ").push_bind(limit);
        qb.push(" OFFSET ").push_bind(offset);
        qb.build_query_as::<StatusEventRow>()
            .fetch_all(self.db.pool())
            .await
    }

    /// 过滤计数（镜像 list_paged 的全部 WHERE 分支）。
    pub async fn count_filtered(
        &self,
        q: Option<&str>,
        host_id: Option<&str>,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> sqlx::Result<i64> {
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT COUNT(*) FROM status_events WHERE 1=1",
        );
        push_filters(&mut qb, q, host_id, from, to);
        let (n,): (i64,) = qb.build_query_as().fetch_one(self.db.pool()).await?;
        Ok(n)
    }
}

/// list_paged / count_filtered 共用的 WHERE 追加逻辑（保证两查询过滤范围一致）。
fn push_filters(
    qb: &mut sqlx::QueryBuilder<sqlx::Postgres>,
    q: Option<&str>,
    host_id: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) {
    if let Some(kw) = q {
        let pat = format!("%{kw}%");
        qb.push(" AND (reason ILIKE ")
            .push_bind(pat.clone())
            .push(" OR detail ILIKE ")
            .push_bind(pat.clone())
            .push(" OR host_id ILIKE ")
            .push_bind(pat)
            .push(")");
    }
    if let Some(h) = host_id {
        qb.push(" AND host_id = ").push_bind(h.to_string());
    }
    if let Some(f) = from {
        qb.push(" AND created_at >= ").push_bind(f);
    }
    if let Some(t) = to {
        qb.push(" AND created_at <= ").push_bind(t);
    }
}
