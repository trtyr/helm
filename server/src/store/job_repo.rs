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
    /// 幂等守卫（EN-64/B4）：终态写入后不再变更——agent 重放/迟到回报不会覆盖终态。
    pub async fn set_status(&self, id: Uuid, status: &str) -> sqlx::Result<()> {
        sqlx::query(
            "UPDATE jobs SET status = $2,
                 started_at = COALESCE(started_at, CASE WHEN $2 = 'running' THEN now() END),
                 finished_at = CASE WHEN $2 IN ('succeeded','failed','timed_out','cancelled') THEN now() ELSE finished_at END
             WHERE id = $1
               AND status NOT IN ('succeeded','failed','timed_out','cancelled')",
        )
        .bind(id)
        .bind(status)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 落最终结果：状态 + 输出 + 退出码。
    /// 幂等守卫（EN-64/B4）：已有终态时拒绝写入（返回 false）——重放/迟到回报不覆盖终态。
    pub async fn finish(
        &self,
        id: Uuid,
        status: &str,
        output: &str,
        exit_code: Option<i32>,
    ) -> sqlx::Result<bool> {
        let result = sqlx::query(
            "UPDATE jobs SET status = $2, output = $3, exit_code = $4, finished_at = now()
             WHERE id = $1
               AND status NOT IN ('succeeded','failed','timed_out','cancelled')",
        )
        .bind(id)
        .bind(status)
        .bind(output)
        .bind(exit_code)
        .execute(self.db.pool())
        .await?;
        Ok(result.rows_affected() > 0)
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

    /// 分页列出 job（按 started_at 倒序）。
    pub async fn list_paged(
        &self,
        limit: i64,
        offset: i64,
        sort: Option<(String, bool)>,
    ) -> sqlx::Result<Vec<JobRow>> {
        self.list_filtered(None, None, None, None, limit, offset, sort)
            .await
    }

    /// 过滤列出（D4）：按 status / host_id 可选过滤，全为 None 时等价 list_paged。
    /// sort（P001-T1c）：`(field, desc)`——field 必须命中白名单（store::order_by），未命中回退默认。
    /// 过滤条件下的总数（P001-T4 分页栏 total）。镜像 list_filtered 的全部 WHERE 分支。
    pub async fn count_filtered(
        &self,
        status: Option<&str>,
        host_id: Option<Uuid>,
        from: Option<chrono::DateTime<Utc>>,
        to: Option<chrono::DateTime<Utc>>,
    ) -> sqlx::Result<i64> {
        let mut qb =
            sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT COUNT(*) FROM jobs WHERE 1=1");
        if let Some(s) = status {
            qb.push(" AND status = ").push_bind(s.to_string());
        }
        if let Some(h) = host_id {
            qb.push(" AND host_id = ").push_bind(h);
        }
        if let Some(f) = from {
            qb.push(" AND created_at >= ").push_bind(f);
        }
        if let Some(t) = to {
            qb.push(" AND created_at <= ").push_bind(t);
        }
        let (n,): (i64,) = qb.build_query_as().fetch_one(self.db.pool()).await?;
        Ok(n)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_filtered(
        &self,
        status: Option<&str>,
        host_id: Option<Uuid>,
        from: Option<chrono::DateTime<Utc>>,
        to: Option<chrono::DateTime<Utc>>,
        limit: i64,
        offset: i64,
        sort: Option<(String, bool)>,
    ) -> sqlx::Result<Vec<JobRow>> {
        let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
            "SELECT id, task_id, host_id, status, command, args, output, exit_code,
                    started_at, finished_at FROM jobs WHERE 1=1",
        );
        if let Some(s) = status {
            qb.push(" AND status = ").push_bind(s.to_string());
        }
        if let Some(h) = host_id {
            qb.push(" AND host_id = ").push_bind(h);
        }
        if let Some(f) = from {
            qb.push(" AND created_at >= ").push_bind(f);
        }
        if let Some(t) = to {
            qb.push(" AND created_at <= ").push_bind(t);
        }
        let (field, desc) = sort
            .as_ref()
            .map(|(f, d)| (f.as_str(), *d))
            .unwrap_or(("started_at", true));
        let order = super::order_by(
            field,
            desc,
            &[
                ("started_at", "started_at {dir} NULLS LAST"),
                ("status", "status {dir}"),
                ("host_id", "host_id {dir}"),
                ("exit_code", "exit_code {dir}"),
                ("command", "command {dir}"),
            ],
            "started_at DESC NULLS LAST",
        );
        qb.push(" ORDER BY ")
            .push(order)
            .push(" LIMIT ")
            .push_bind(limit)
            .push(" OFFSET ")
            .push_bind(offset);
        qb.build_query_as::<JobRow>()
            .fetch_all(self.db.pool())
            .await
    }

    /// sweeper（EN-64）：将 running 超过 `timeout_secs` 的 job 置 timed_out。
    /// 返回 (job_id, 最近注册的 agent_id)——agent 在线时 sweeper 顺带补发 JobCancel。
    pub async fn expire_running(&self, timeout_secs: i64) -> sqlx::Result<Vec<ExpiredJob>> {
        sqlx::query_as::<_, ExpiredJob>(
            "UPDATE jobs SET status = 'timed_out', finished_at = now()
             WHERE status = 'running'
               AND started_at IS NOT NULL
               AND started_at < now() - make_interval(secs => $1)
             RETURNING id,
                       (SELECT a.id FROM agents a
                        WHERE a.host_id = jobs.host_id
                        ORDER BY a.registered_at DESC LIMIT 1) AS agent_id",
        )
        .bind(timeout_secs)
        .fetch_all(self.db.pool())
        .await
    }

    /// sweeper（EN-67）：将 queued 超过 `timeout_secs` 的孤行置 failed，返回 job_id。
    pub async fn fail_stale_queued(&self, timeout_secs: i64) -> sqlx::Result<Vec<Uuid>> {
        sqlx::query_as::<_, (Uuid,)>(
            "UPDATE jobs SET status = 'failed', finished_at = now()
             WHERE status = 'queued'
               AND created_at < now() - make_interval(secs => $1)
             RETURNING id",
        )
        .bind(timeout_secs)
        .fetch_all(self.db.pool())
        .await
        .map(|rows| rows.into_iter().map(|r| r.0).collect())
    }

    /// 记录离线取消补偿（agent 重连后补发 JobCancel；同 (agent, job) 幂等）。
    pub async fn mark_cancel_pending(&self, agent_id: &str, job_id: Uuid) -> sqlx::Result<()> {
        sqlx::query(
            "INSERT INTO job_cancel_pending (agent_id, job_id) VALUES ($1, $2)
             ON CONFLICT (agent_id, job_id) DO NOTHING",
        )
        .bind(agent_id)
        .bind(job_id)
        .execute(self.db.pool())
        .await?;
        Ok(())
    }

    /// 取出并清除该 agent 的全部挂起取消（重连瞬间调用）。
    pub async fn take_pending_cancels(&self, agent_id: &str) -> sqlx::Result<Vec<Uuid>> {
        sqlx::query_as::<_, (Uuid,)>(
            "DELETE FROM job_cancel_pending WHERE agent_id = $1 RETURNING job_id",
        )
        .bind(agent_id)
        .fetch_all(self.db.pool())
        .await
        .map(|rows| rows.into_iter().map(|r| r.0).collect())
    }

    /// retention（C1）：删除 `cutoff` 之前的 job 行，返回 job_id 列表。
    pub async fn delete_before(&self, cutoff: DateTime<Utc>) -> sqlx::Result<Vec<Uuid>> {
        sqlx::query_as::<_, (Uuid,)>("DELETE FROM jobs WHERE created_at < $1 RETURNING id")
            .bind(cutoff)
            .fetch_all(self.db.pool())
            .await
            .map(|rows| rows.into_iter().map(|r| r.0).collect())
    }
}

/// sweeper 过期结果：job id + 该 host 最近注册的 agent（可空——agent 可能已被注销）。
#[derive(Debug, FromRow)]
pub struct ExpiredJob {
    pub id: Uuid,
    pub agent_id: Option<String>,
}
