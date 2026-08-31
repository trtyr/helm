//! Job 仓储：jobs 表的读写。

use crate::store::Db;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

/// `jobs` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct JobRow {
    pub id: Uuid,
    pub task_id: Option<Uuid>,
    pub host_id: Uuid,
    pub status: String,
    pub command: String,
    pub args: Vec<String>,
    pub output: Option<String>,
    pub exit_code: Option<i32>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

/// jobs 表仓储。
#[derive(Clone)]
pub struct JobRepo {
    db: Db,
}

impl JobRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 新建 Job（status = queued）。
    pub async fn create(
        &self,
        host_id: Uuid,
        command: &str,
        args: &[String],
    ) -> sqlx::Result<JobRow> {
        sqlx::query_as::<_, JobRow>(
            "INSERT INTO jobs (host_id, command, args, status)
             VALUES ($1, $2, $3, 'queued')
             RETURNING id, task_id, host_id, status, command, args, output,
                       exit_code, started_at, finished_at",
        )
        .bind(host_id)
        .bind(command)
        .bind(args)
        .fetch_one(self.db.pool())
        .await
    }

    /// 更新状态（running 时补 started_at，终态补 finished_at）。
    pub async fn set_status(&self, id: Uuid, status: &str) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE jobs SET status = $2,
                 started_at = COALESCE(started_at, CASE WHEN $2 = 'running' THEN now() END),
                 finished_at = CASE WHEN $2 IN ('succeeded','failed','timed_out','cancelled') THEN now() ELSE finished_at END
             WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 落最终结果：状态 + 输出 + 退出码。
    pub async fn finish(
        &self,
        id: Uuid,
        status: &str,
        output: &str,
        exit_code: Option<i32>,
    ) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE jobs SET status = $2, output = $3, exit_code = $4, finished_at = now() WHERE id = $1",
        )
        .bind(id)
        .bind(status)
        .bind(output)
        .bind(exit_code)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 按 id 查询。
    pub async fn get(&self, id: Uuid) -> sqlx::Result<Option<JobRow>> {
        sqlx::query_as::<_, JobRow>(
            "SELECT id, task_id, host_id, status, command, args, output, exit_code,
                    started_at, finished_at FROM jobs WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }
}
