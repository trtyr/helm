//! FileTransfer 仓储：file_transfers 表的读写。

use crate::store::Db;
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
