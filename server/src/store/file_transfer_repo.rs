//! FileTransfer 仓储：file_transfers 表的读写。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `file_transfers` 表行。
#[derive(Debug, Clone, FromRow)]
pub struct FileTransferRow {
    pub id: Uuid,
    pub host_id: Uuid,
    pub direction: String,
    pub path: String,
    pub size: i64,
    pub bytes_transferred: i64,
    pub checksum: Option<String>,
    pub status: String,
}

/// file_transfers 表仓储。
#[derive(Clone)]
pub struct FileTransferRepo {
    db: Db,
}

impl FileTransferRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 新建文件传输记录（status = pending）。
    pub async fn create(
        &self,
        host_id: Uuid,
        direction: &str,
        path: &str,
        size: i64,
    ) -> sqlx::Result<FileTransferRow> {
        sqlx::query_as::<_, FileTransferRow>(
            "INSERT INTO file_transfers (host_id, direction, path, size)
             VALUES ($1, $2, $3, $4)
             RETURNING id, host_id, direction, path, size, bytes_transferred, checksum, status",
        )
        .bind(host_id)
        .bind(direction)
        .bind(path)
        .bind(size)
        .fetch_one(self.db.pool())
        .await
    }

    /// 按 id 读单条传输记录（不存在返回 None）。
    pub async fn find(&self, id: Uuid) -> sqlx::Result<Option<FileTransferRow>> {
        sqlx::query_as::<_, FileTransferRow>(
            "SELECT id, host_id, direction, path, size, bytes_transferred, checksum, status
             FROM file_transfers WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 重启对账（T007）：把上一进程遗留的非终态传输（`pending` / `transferring`）
    /// 置为 `failed` 并落 `finished_at`，返回受影响 id。
    ///
    /// 表里没有「原因」列（见 0001 迁移），原因由调用方记账（日志/审计）——
    /// 语义与 job 侧一致：服务重启导致的中断不是传输失败本身，但终态必须明确。
    pub async fn fail_orphans(&self) -> sqlx::Result<Vec<Uuid>> {
        sqlx::query_as::<_, (Uuid,)>(
            "UPDATE file_transfers SET status = 'failed', finished_at = now()
              WHERE status IN ('pending', 'transferring')
              RETURNING id",
        )
        .fetch_all(self.db.pool())
        .await
        .map(|rows| rows.into_iter().map(|r| r.0).collect())
    }

    /// retention（C1）：删除 `cutoff` 之前的传输元数据行，返回删除行数。
    pub async fn delete_before(&self, cutoff: DateTime<Utc>) -> sqlx::Result<u64> {
        let result = sqlx::query("DELETE FROM file_transfers WHERE created_at < $1")
            .bind(cutoff)
            .execute(self.db.pool())
            .await?;
        Ok(result.rows_affected())
    }

    /// 落最终状态与校验和。
    pub async fn finish(
        &self,
        id: Uuid,
        status: &str,
        bytes: i64,
        checksum: &str,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE file_transfers
             SET status = $2, bytes_transferred = $3, checksum = $4, finished_at = now()
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(bytes)
        .bind(checksum)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }
}
