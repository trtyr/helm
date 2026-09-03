//! Notification 仓储：notifications 表的读写（系统内通知中心，决策 009）。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `notifications` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct NotificationRow {
    pub id: Uuid,
    pub host_id: Uuid,
    /// 通知类型：online / offline / alert（SQL 列名 `type`）。
    #[sqlx(rename = "type")]
    pub kind: String,
    pub message: String,
    pub read: bool,
    pub created_at: DateTime<Utc>,
}

/// notifications 表仓储。
#[derive(Clone)]
pub struct NotificationRepo {
    db: Db,
}

impl NotificationRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 插入一条通知。
    pub async fn insert(
        &self,
        host_id: Uuid,
        kind: &str,
        message: &str,
    ) -> sqlx::Result<NotificationRow> {
        sqlx::query_as::<_, NotificationRow>(
            "INSERT INTO notifications (host_id, type, message)
             VALUES ($1, $2, $3)
             RETURNING id, host_id, type, message, read, created_at",
        )
        .bind(host_id)
        .bind(kind)
        .bind(message)
        .fetch_one(self.db.pool())
        .await
    }

    /// 冷却窗口内刷新已有通知：更新消息与时间、重置为未读（新事件发生）。
    pub async fn refresh(&self, id: Uuid, message: &str) -> sqlx::Result<NotificationRow> {
        sqlx::query_as::<_, NotificationRow>(
            "UPDATE notifications
             SET message = $2, created_at = now(), read = FALSE
             WHERE id = $1
             RETURNING id, host_id, type, message, read, created_at",
        )
        .bind(id)
        .bind(message)
        .fetch_one(self.db.pool())
        .await
    }

    /// 某 host 某类型最近的一条通知（冷却窗口判定用）。
    pub async fn latest_of_type(
        &self,
        host_id: Uuid,
        kind: &str,
    ) -> sqlx::Result<Option<NotificationRow>> {
        sqlx::query_as::<_, NotificationRow>(
            "SELECT id, host_id, type, message, read, created_at
             FROM notifications
             WHERE host_id = $1 AND type = $2
             ORDER BY created_at DESC
             LIMIT 1",
        )
        .bind(host_id)
        .bind(kind)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 分页列出通知（按时间倒序；`unread_only` 只看未读）。
    pub async fn list_paged(
        &self,
        limit: i64,
        offset: i64,
        unread_only: bool,
    ) -> sqlx::Result<Vec<NotificationRow>> {
        let sql = if unread_only {
            "SELECT id, host_id, type, message, read, created_at
             FROM notifications WHERE NOT read
             ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        } else {
            "SELECT id, host_id, type, message, read, created_at
             FROM notifications
             ORDER BY created_at DESC LIMIT $1 OFFSET $2"
        };
        sqlx::query_as::<_, NotificationRow>(sql)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
    }

    /// 计数（`unread_only` 为 true 时计未读数）。
    pub async fn count(&self, unread_only: bool) -> sqlx::Result<i64> {
        let sql = if unread_only {
            "SELECT COUNT(*) FROM notifications WHERE NOT read"
        } else {
            "SELECT COUNT(*) FROM notifications"
        };
        sqlx::query_scalar(sql).fetch_one(self.db.pool()).await
    }

    /// 标记单条已读。返回是否命中。
    pub async fn mark_read(&self, id: Uuid) -> sqlx::Result<bool> {
        sqlx::query("UPDATE notifications SET read = TRUE WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected() > 0)
    }

    /// 全部标记已读。返回影响行数。
    pub async fn mark_all_read(&self) -> sqlx::Result<u64> {
        sqlx::query("UPDATE notifications SET read = TRUE WHERE NOT read")
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected())
    }

    /// 删除指定时间之前的通知（时序保留，与 metrics/alerts 一致由后台任务调用）。
    pub async fn delete_before(&self, cutoff: DateTime<Utc>) -> sqlx::Result<u64> {
        sqlx::query("DELETE FROM notifications WHERE created_at < $1")
            .bind(cutoff)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected())
    }
}
