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

    // 离线签发模式：签发 agent 证书三件套后退出（forward 预置分发用）
    if config.issue_cert {
        if config.issue_agent_id.is_empty() || config.issue_out_dir.is_empty() {
            anyhow::bail!("--issue-cert 需要 --issue-agent-id 与 --issue-out-dir");
        }
        let cert = application::cert_service::CertService::load_or_generate(
            &config.tls_dir,
            &config.tls_server_name,
            config.mtls,
        )?;
        cert.issue_agent_cert(
            &config.issue_agent_id,
            &config.issue_san,
            &config.issue_out_dir,
        )?;
        tracing::info!(
            agent_id = %config.issue_agent_id,
            out_dir = %config.issue_out_dir,
            "issue-cert done"
        );
        return Ok(());
    }

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
    let listeners = grpc::listener_registry::ListenerRegistry::new();
    let sessions = grpc::session_registry::SessionRegistry::new();
    let file_list = grpc::file_list_registry::FileListRegistry::new();
    let query = grpc::query_registry::QueryRegistry::new();
    let streams = grpc::stream_registry::StreamRegistry::new();
    let cert = application::cert_service::CertService::load_or_generate(
        &config.tls_dir,
        &config.tls_server_name,
        config.mtls,
    )?;

    let forward_deps = grpc::forward_manager::ForwardDeps {
        registry: registry.clone(),
        transfers: transfers.clone(),
        sessions: sessions.clone(),
        file_list: file_list.clone(),
        query: query.clone(),
        streams: streams.clone(),
        db: db.clone(),
        server_token: config.server_token.clone(),
        cert: cert.clone(),
        tls_server_name: config.tls_server_name.clone(),
    };
    grpc::forward_manager::ForwardManager::new().spawn_reconciler(forward_deps);

    // 兜底下线扫描：心跳超时的漏网场景（半开连接）补发下线通知（决策 009）
    application::notification_service::spawn_offline_sweeper(
        db.clone(),
        streams.clone(),
        registry.clone(),
        config.heartbeat_timeout_secs,
    );

    // Job 超时兜底扫描（EN-64/EN-67）：running 超龄置 timed_out（在线补发 Cancel）、
    // queued 孤行置 failed；HELM_JOB_TIMEOUT_SECS=0 禁用
    application::job_sweeper::spawn_job_sweeper(
        db.clone(),
        registry.clone(),
        config.job_timeout_secs,
    );

    // 恢复已落库的定时任务
    let exec = application::exec_service::ExecService::new(db.clone(), registry.clone());
    application::scheduler::resume_scheduled(db.clone(), exec).await?;

    // 恢复/初始化监听器（首次启动 seed 默认监听器）
    application::listener_service::ListenerService::new(
        db.clone(),
        listeners.clone(),
        registry.clone(),
        transfers.clone(),
        sessions.clone(),
        file_list.clone(),
        query.clone(),
        streams.clone(),
        config.server_token.clone(),
        cert.clone(),
    )
    .resume_or_seed(&config.grpc_addr)
    .await?;

    // 时序保留清理：后台每 24h 删除 30 天前的 metrics + alerts + notifications + 已吊销 api_keys
    let cleanup_db = db.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
            let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
            let m = store::metric_repo::MetricRepo::new(cleanup_db.clone())
                .delete_before(cutoff)
                .await;
            let a = store::alert_repo::AlertRepo::new(cleanup_db.clone())
                .delete_before(cutoff)
                .await;
            let n = store::notification_repo::NotificationRepo::new(cleanup_db.clone())
                .delete_before(cutoff)
                .await;
            let k = store::api_key_repo::ApiKeyRepo::new(cleanup_db.clone())
                .delete_revoked_before(cutoff)
                .await;
            tracing::info!(metrics_deleted = ?m, alerts_deleted = ?a, notifications_deleted = ?n, api_keys_deleted = ?k, "retention cleanup");
        }
    });

    http::serve(
        config.clone(),
        db.clone(),
        registry.clone(),
        transfers.clone(),
        listeners.clone(),
        sessions.clone(),
        file_list.clone(),
        query.clone(),
        streams.clone(),
        cert.clone(),
    )
    .await?;

    Ok(())
}
