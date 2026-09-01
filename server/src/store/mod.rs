//! 持久化适配层：Postgres 连接 + 迁移 + 仓储。
//!
//! 所有数据库访问经 [`Db`] 聚合根；仓储按实体拆分。

pub mod agent_repo;
pub mod file_transfer_repo;
pub mod host_repo;
pub mod job_repo;
pub mod listener_repo;
pub mod metric_repo;
pub mod task_repo;
pub mod user_repo;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

/// 数据库访问聚合根。`Clone` 后共享底层连接池。
#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// 建立连接池。
    pub async fn connect(url: &str) -> sqlx::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(url)
            .await?;
        Ok(Self { pool })
    }

    /// 执行编译期嵌入的迁移（`server/migrations`）。
    pub async fn migrate(&self) -> sqlx::Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }

    /// 底层连接池（仓储内部使用）。
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
