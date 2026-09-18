//! 持久化适配层：Postgres 连接 + 迁移 + 仓储。
//!
//! 所有数据库访问经 [`Db`] 聚合根；仓储按实体拆分。

pub mod agent_repo;
pub mod alert_repo;
pub mod api_key_repo;
pub mod audit_repo;
pub mod file_transfer_repo;
pub mod host_repo;
pub mod ir_repo;
pub mod job_repo;
pub mod listener_repo;
pub mod metric_repo;
pub mod notification_repo;
pub mod service_repo;
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
    /// 建立连接池（默认：容量 10、获取超时 30s）。
    pub async fn connect(url: &str) -> sqlx::Result<Self> {
        Self::connect_with(url, 10, 30).await
    }

    /// 建立连接池（C3：池容量与获取超时可配，避免默认 10 连接被多场景共享时无界排队）。
    pub async fn connect_with(
        url: &str,
        max_connections: u32,
        acquire_timeout_secs: u64,
    ) -> sqlx::Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections.max(1))
            .acquire_timeout(std::time::Duration::from_secs(acquire_timeout_secs.max(1)))
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
