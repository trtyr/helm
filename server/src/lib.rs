pub mod application;
pub mod config;
pub mod domain;
pub mod grpc;
pub mod http;
pub mod store;
pub mod telemetry;

use anyhow::Result;

/// 服务入口：加载配置、初始化可观测性、连接数据库并执行迁移、启动 HTTP。
///
/// **G11 拆分（2026-09-21）**：本函数原为 192 行单块（配置 + 日志 + 数据库 + 注册表 +
/// 六个后台任务 + 三个恢复步骤 + HTTP 全在一处）。现按启动阶段拆为具名函数——
/// `issue_agent_cert` / `init_telemetry` / `init_database` / [`ServiceDeps::new`] /
/// `spawn_background_tasks` / `resume_persisted_state`；本函数只做阶段编排，
/// 启动顺序即阅读顺序（顺序本身有语义，见各阶段的注释）。
pub async fn run() -> Result<()> {
    let config = config::Config::load()?;

    // A1：弱默认凭据守卫——命中即 ERROR；HELM_REQUIRE_STRONG_DEFAULTS=true 时拒绝启动
    config.guard_insecure_defaults()?;

    // 离线签发模式：签发 agent 证书三件套后退出（forward 预置分发用）
    if config.issue_cert {
        return issue_agent_cert(&config);
    }

    // 阶段 1：日志落盘（guard 须保活到进程退出）
    let _log_guard = init_telemetry(&config)?;

    // 阶段 2：数据库连接 + 迁移 + 默认账号
    let db = init_database(&config).await?;

    // 阶段 3：注册表 / 指标队列 / 证书
    let deps = ServiceDeps::new(db, &config)?;

    // 阶段 4：后台任务（forward 拨号、四个扫描器、保留清理）
    spawn_background_tasks(&deps, &config);

    // 阶段 5：恢复已落库的业务状态（定时任务 + 监听器）
    resume_persisted_state(&deps, &config).await?;

    // 阶段 6：HTTP 服务（阻塞至进程退出）
    http::serve(deps.into_serve_deps(config)).await?;
    Ok(())
}

/// 离线签发分支：签发 agent 证书三件套后直接返回（不启动服务）。
fn issue_agent_cert(config: &config::Config) -> Result<()> {
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
    Ok(())
}

/// 阶段 1：日志落盘（P003 T3）——stdout + 按天轮转双写。
///
/// 返回的 guard 必须由调用方**保活到进程退出**：非阻塞 appender 的写入线程随 guard
/// 一起 drop，提前释放会静默丢日志。
fn init_telemetry(
    config: &config::Config,
) -> Result<Option<tracing_appender::non_blocking::WorkerGuard>> {
    let log_dir = if config.log_dir.is_empty() {
        None
    } else {
        let d = std::path::PathBuf::from(&config.log_dir);
        std::fs::create_dir_all(&d)?;
        Some(d)
    };
    let guard = telemetry::init(&config.log_level, log_dir.as_ref());
    if let Some(d) = &log_dir {
        telemetry::spawn_log_retention(d.clone(), config.retention_days);
    }
    tracing::info!(
        http_addr = %config.http_addr,
        grpc_addr = %config.grpc_addr,
        "helm-server starting"
    );
    Ok(guard)
}

/// 阶段 2：连接数据库、执行迁移、确保默认账号存在（A1 的种子账号）。
async fn init_database(config: &config::Config) -> Result<store::Db> {
    let db = store::Db::connect_with(
        &config.database_url,
        config.db_max_connections,
        config.db_acquire_timeout_secs,
    )
    .await?;
    db.migrate().await?;
    tracing::info!("database connected and migrated");

    application::auth_service::AuthService::new(db.clone(), config.jwt_secret.clone())
        .seed_admin()
        .await?;
    Ok(db)
}

/// 阶段 3 产出：启动期构建、运行期共享的依赖集合（全为 Arc/句柄，Clone 便宜）。
struct ServiceDeps {
    db: store::Db,
    registry: grpc::connection_registry::ConnectionRegistry,
    transfers: grpc::transfer_registry::TransferRegistry,
    listeners: grpc::listener_registry::ListenerRegistry,
    sessions: grpc::session_registry::SessionRegistry,
    file_list: grpc::file_list_registry::FileListRegistry,
    query: grpc::query_registry::QueryRegistry,
    streams: grpc::stream_registry::StreamRegistry,
    metrics: application::metric_sink::MetricSink,
    cert: application::cert_service::CertService,
}

impl ServiceDeps {
    /// 构建全部注册表、指标队列与证书服务（证书在启动期一次性加载或生成）。
    fn new(db: store::Db, config: &config::Config) -> Result<Self> {
        let registry = grpc::connection_registry::ConnectionRegistry::new();
        let transfers = grpc::transfer_registry::TransferRegistry::new();
        let listeners = grpc::listener_registry::ListenerRegistry::new();
        let sessions = grpc::session_registry::SessionRegistry::new();
        let file_list = grpc::file_list_registry::FileListRegistry::new();
        let query = grpc::query_registry::QueryRegistry::new();
        let streams = grpc::stream_registry::StreamRegistry::new();
        // 指标落库队列（E2）：MetricReport 异步批量落库，不阻塞结果类消息
        let metrics = application::metric_sink::MetricSink::spawn(db.clone(), streams.clone());
        let cert = application::cert_service::CertService::load_or_generate(
            &config.tls_dir,
            &config.tls_server_name,
            config.mtls,
        )?;
        Ok(Self {
            db,
            registry,
            transfers,
            listeners,
            sessions,
            file_list,
            query,
            streams,
            metrics,
            cert,
        })
    }

    /// 阶段 6 的入参：按值移动给 HTTP 层（此后不再使用）。
    fn into_serve_deps(self, config: config::Config) -> http::HttpServeDeps {
        http::HttpServeDeps {
            config,
            db: self.db,
            registry: self.registry,
            transfers: self.transfers,
            listeners: self.listeners,
            sessions: self.sessions,
            file_list: self.file_list,
            query: self.query,
            streams: self.streams,
            metrics: self.metrics,
            cert: self.cert,
        }
    }
}

/// 阶段 4：启动全部后台任务（各自循环，无需 join 句柄）。
fn spawn_background_tasks(deps: &ServiceDeps, config: &config::Config) {
    // forward 模式拨号管理器：对照 hosts 表差分启停持久连接
    let forward_deps = grpc::forward_manager::ForwardDeps {
        registry: deps.registry.clone(),
        transfers: deps.transfers.clone(),
        sessions: deps.sessions.clone(),
        file_list: deps.file_list.clone(),
        query: deps.query.clone(),
        streams: deps.streams.clone(),
        metrics: deps.metrics.clone(),
        db: deps.db.clone(),
        server_tokens: config.accepted_server_tokens(),
        cert: deps.cert.clone(),
        tls_server_name: config.tls_server_name.clone(),
    };
    grpc::forward_manager::ForwardManager::new().spawn_reconciler(forward_deps);

    // 兜底下线扫描：心跳超时的漏网场景（半开连接）补发下线通知（决策 009）
    application::notification_service::spawn_offline_sweeper(
        deps.db.clone(),
        deps.streams.clone(),
        deps.registry.clone(),
        config.heartbeat_timeout_secs,
    );

    // Job 超时兜底扫描（EN-64/EN-67）：running 超龄置 timed_out（在线补发 Cancel）、
    // queued 孤行置 failed；HELM_JOB_TIMEOUT_SECS=0 禁用
    application::job_sweeper::spawn_job_sweeper(
        deps.db.clone(),
        deps.registry.clone(),
        config.job_timeout_secs,
    );

    // pending_offline TTL 清理（F1）：挂起下线/注销超 7 天无人认领则作废并告警
    application::job_sweeper::spawn_pending_offline_ttl_sweeper(deps.db.clone());

    // 离线升级告警（P003 T2）：offline 持续超阈值升级 alerts；HELM_OFFLINE_ALERT_MINS=0 禁用
    application::offline_alert_sweeper::spawn_offline_alert_sweeper(
        deps.db.clone(),
        config.offline_alert_mins,
    );

    spawn_retention_cleanup(deps.db.clone(), config.retention_days);
}

/// 数据保留清理（C1）：每 24h 删除超过 `HELM_RETENTION_DAYS`（默认 90 天）的时序类
/// （metrics/alerts/notifications）+ 已吊销 api_keys + 执行历史（jobs/audit_logs/file_transfers）；
/// IR 表不自动清理（取证数据需显式策略）。
fn spawn_retention_cleanup(db: store::Db, retention_days: i64) {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(24 * 3600)).await;
            let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days);
            let m = store::metric_repo::MetricRepo::new(db.clone())
                .delete_before(cutoff)
                .await;
            let a = store::alert_repo::AlertRepo::new(db.clone())
                .delete_before(cutoff)
                .await;
            let n = store::notification_repo::NotificationRepo::new(db.clone())
                .delete_before(cutoff)
                .await;
            let k = store::api_key_repo::ApiKeyRepo::new(db.clone())
                .delete_revoked_before(cutoff)
                .await;
            let j = store::job_repo::JobRepo::new(db.clone())
                .delete_before(cutoff)
                .await;
            let al = store::audit_repo::AuditRepo::new(db.clone())
                .delete_before(cutoff)
                .await;
            let f = store::file_transfer_repo::FileTransferRepo::new(db.clone())
                .delete_before(cutoff)
                .await;
            tracing::info!(
                metrics_deleted = ?m, alerts_deleted = ?a, notifications_deleted = ?n,
                api_keys_deleted = ?k, jobs_deleted = ?j, audit_deleted = ?al,
                file_transfers_deleted = ?f, retention_days, "retention cleanup"
            );
        }
    });
}

/// 阶段 5：恢复已落库的业务状态（定时任务 + 监听器）。
async fn resume_persisted_state(deps: &ServiceDeps, config: &config::Config) -> Result<()> {
    // 恢复已落库的定时任务
    let exec = application::exec_service::ExecService::new(deps.db.clone(), deps.registry.clone());
    application::scheduler::resume_scheduled(deps.db.clone(), exec).await?;

    // 恢复/初始化监听器（首次启动 seed 默认监听器）
    application::listener_service::ListenerService::new(
        deps.db.clone(),
        deps.listeners.clone(),
        deps.registry.clone(),
        deps.transfers.clone(),
        deps.sessions.clone(),
        deps.file_list.clone(),
        deps.query.clone(),
        deps.streams.clone(),
        deps.metrics.clone(),
        config.accepted_server_tokens(),
        deps.cert.clone(),
    )
    .resume_or_seed(&config.grpc_addr)
    .await?;
    Ok(())
}
