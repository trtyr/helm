//! 集成测试共享夹具：专用临时数据库，与 dev 库完全隔离。
//!
//! 全进程首次取库时重建 `helm_itest`（`DROP ... WITH (FORCE)` 强断旧连接，
//! PG 13+），随后迁移。断言可依赖空库计数（如 insert 后 delete_before 应恰好
//! 删 1 条），不会触碰也不会被 dev 库的存量数据影响。库保留到下次运行时
//! 重建，固定名字，不堆积。
//!
//! 注意：sqlx 连接池不可跨 tokio runtime 共享（每个 #[tokio::test] 是独立
//! runtime，先建池会随其 runtime 销毁，后续测试 PoolTimedOut），因此这里只
//! 共享"重建库"这一次性动作，池由各测试自建。

use helm_server::store::Db;

pub const TEST_DB: &str = "helm_itest";

/// 连接串 origin（去掉末尾库名）。默认 docker compose 映射的 5433。
fn origin() -> String {
    let url = std::env::var("HELM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm".to_string());
    match url.rfind('/') {
        Some(i) => url[..i].to_string(),
        None => url,
    }
}

pub fn test_url() -> String {
    format!("{}/{}", origin(), TEST_DB)
}

/// 库重建只做一次的栅栏。
static PROVISIONED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

/// 取测试库连接：确保库已重建+迁移，然后为当前测试独立建池。
pub async fn connect() -> Db {
    PROVISIONED.get_or_init(provision).await;
    Db::connect(&test_url()).await.expect("connect test db")
}

async fn provision() {
    let admin = Db::connect(&format!("{}/postgres", origin()))
        .await
        .expect("connect admin db");
    // DDL 不能预编译（CREATE DATABASE 拒绝事务/参数绑定），走 raw_sql 简单查询；
    // 库名是编译期常量、无注入面，显式 AssertSqlSafe 过审计
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {TEST_DB} WITH (FORCE)"
    )))
    .execute(admin.pool())
    .await
    .expect("drop stale test db");
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE DATABASE {TEST_DB}")))
        .execute(admin.pool())
        .await
        .expect("create test db");
    admin.pool().close().await;

    let db = Db::connect(&test_url()).await.expect("connect test db");
    db.migrate().await.expect("migrate test db");
    db.pool().close().await;
}
