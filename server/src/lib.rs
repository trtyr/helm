pub mod application;
pub mod config;
pub mod domain;
pub mod grpc;
pub mod http;
pub mod store;
pub mod telemetry;

use anyhow::Result;

/// 服务入口：加载配置、初始化可观测性、连接数据库并执行迁移、启动 HTTP。
pub async fn run() -> Result<()> {
    let config = config::Config::load()?;
    telemetry::init(&config.log_level);

    tracing::info!(
        http_addr = %config.http_addr,
        grpc_addr = %config.grpc_addr,
        "helm-server starting"
    );

    let db = store::Db::connect(&config.database_url).await?;
    db.migrate().await?;
    tracing::info!("database connected and migrated");

    application::auth_service::AuthService::new(db.clone(), config.jwt_secret.clone())
        .seed_admin()
        .await?;

    let registry = grpc::connection_registry::ConnectionRegistry::new();
    let transfers = grpc::transfer_registry::TransferRegistry::new();

    tokio::try_join!(
        http::serve(
            config.clone(),
            db.clone(),
            registry.clone(),
            transfers.clone()
        ),
        grpc::serve(config, db, registry, transfers),
    )?;

    Ok(())
}
