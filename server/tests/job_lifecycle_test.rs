//! Job 生命周期集成测试（EN-64 超时/取消 + EN-67 queued 孤行回收）。
//!
//! 不需要真实 agent：ConnectionRegistry 注册内存假通道承接 JobCancel 下发，
//! sweeper 直接驱动 `sweep_once`（不经 30s 周期）。

mod common;

use helm_proto::pb::server_message;
use helm_server::application::exec_service::ExecService;
use helm_server::application::job_sweeper::sweep_once;
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::store::agent_repo::{AgentRepo, HostOsDetails};
use helm_server::store::job_repo::{JobRepo, JobRow};
use tokio::sync::mpsc;

/// sweeper 的 UPDATE 是全库范围（status + 超龄），并行测试会互相扫走对方的 fixture——
/// 三个 sweeper 测试用这把锁串行。
static SWEEP_GUARD: std::sync::LazyLock<tokio::sync::Mutex<()>> =
    std::sync::LazyLock::new(|| tokio::sync::Mutex::new(()));

/// 注册 agent（DB 行），返回 (agent_id, host_id)。
async fn seed_agent(db: &helm_server::store::Db) -> (String, uuid::Uuid) {
    let agent_id = format!("itest-job-{}", uuid::Uuid::new_v4());
    let hostname = format!("itest-host-{}", uuid::Uuid::new_v4());
    AgentRepo::new(db.clone())
        .register(
            &agent_id,
            "0.0.0",
            &hostname,
            "linux",
            "x86_64",
            "linux-x86_64",
            "203.0.113.7",
            &[],
            false,
            HostOsDetails::default(),
        )
        .await
        .expect("register agent");
    let host_id = AgentRepo::new(db.clone())
        .get_host_id(&agent_id)
        .await
        .expect("get_host_id")
        .expect("host registered");
    (agent_id, host_id)
}

/// 按主键直查 job 行（绕过 repo 便捷层，断言库内真实状态）。
async fn get_job_raw(db: &helm_server::store::Db, id: uuid::Uuid) -> JobRow {
    sqlx::query_as::<_, JobRow>(
        "SELECT id, task_id, host_id, status, command, args, output, exit_code,
                started_at, finished_at FROM jobs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(db.pool())
    .await
    .expect("job row exists")
}

/// 把 started_at / created_at 拨回 `secs` 秒前（模拟超龄 job）。
async fn backdate(db: &helm_server::store::Db, id: uuid::Uuid, secs: i64) {
    sqlx::query("UPDATE jobs SET started_at = now() - make_interval(secs => $1), created_at = now() - make_interval(secs => $1) WHERE id = $2")
        .bind(secs)
        .bind(id)
        .execute(db.pool())
        .await
        .expect("backdate");
}

#[tokio::test]
async fn sweeper_times_out_stale_running() {
    let _guard = SWEEP_GUARD.lock().await;
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let (_agent_id, host_id) = seed_agent(&db).await;

    let job = JobRepo::new(db.clone())
        .create(host_id, "sleep", &["999".to_string()])
        .await
        .expect("create");
    JobRepo::new(db.clone())
        .set_status(job.id, "running")
        .await
        .expect("running");
    backdate(&db, job.id, 3600).await; // running 已 1 小时

    let report = sweep_once(&db, &registry, 300).await;
    assert_eq!(report.expired_running, 1, "stale running must be expired");
    assert_eq!(report.expired_queued, 0);
    assert_eq!(report.cancels_sent, 0, "no live connection registered");

    let row = get_job_raw(&db, job.id).await;
    assert_eq!(row.status, "timed_out", "EN-64: stale running → timed_out");
    assert!(row.finished_at.is_some());
}

#[tokio::test]
async fn sweeper_sends_cancel_to_online_agent() {
    let _guard = SWEEP_GUARD.lock().await;
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let (agent_id, host_id) = seed_agent(&db).await;
    let (msg_tx, mut msg_rx) = mpsc::channel::<helm_proto::pb::ServerMessage>(64);
    registry.register(&agent_id, msg_tx).await;

    let job = JobRepo::new(db.clone())
        .create(host_id, "sleep", &["999".to_string()])
        .await
        .expect("create");
    JobRepo::new(db.clone())
        .set_status(job.id, "running")
        .await
        .expect("running");
    backdate(&db, job.id, 3600).await;

    let report = sweep_once(&db, &registry, 300).await;
    assert_eq!(report.expired_running, 1);
    assert_eq!(
        report.cancels_sent, 1,
        "online agent must receive JobCancel"
    );

    // 假通道收到 JobCancel 帧
    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), msg_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    match msg.kind {
        Some(server_message::Kind::JobCancel(c)) => {
            assert_eq!(c.job_id, job.id.to_string());
        }
        other => panic!("expected JobCancel, got {other:?}"),
    }
}

#[tokio::test]
async fn sweeper_fails_stale_queued_orphans() {
    let _guard = SWEEP_GUARD.lock().await;
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let (_agent_id, host_id) = seed_agent(&db).await;

    // queued 孤行：只 create 不下发（agent 离线时 exec 的遗留形态，EN-67）
    let job = JobRepo::new(db.clone())
        .create(host_id, "whoami", &[])
        .await
        .expect("create");
    backdate(&db, job.id, 3600).await;

    let report = sweep_once(&db, &registry, 300).await;
    assert_eq!(report.expired_queued, 1, "stale queued orphan must fail");
    assert_eq!(report.expired_running, 0);

    let row = get_job_raw(&db, job.id).await;
    assert_eq!(row.status, "failed", "EN-67: stale queued orphan → failed");
}

#[tokio::test]
async fn cancel_queued_job_converges_without_agent() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let (_agent_id, host_id) = seed_agent(&db).await;

    let job = JobRepo::new(db.clone())
        .create(host_id, "uptime", &[])
        .await
        .expect("create");

    let service = ExecService::new(db.clone(), registry);
    let (cancelled, delivered, compensated) = service.cancel(job.id).await.expect("cancel");
    assert!(cancelled);
    assert!(!delivered, "queued job was never sent to agent");
    assert!(!compensated, "queued job has no remote process");

    let row = get_job_raw(&db, job.id).await;
    assert_eq!(row.status, "cancelled");
    assert!(row.finished_at.is_some());
}

#[tokio::test]
async fn cancel_running_online_delivers_job_cancel() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let (agent_id, host_id) = seed_agent(&db).await;
    let (msg_tx, mut msg_rx) = mpsc::channel::<helm_proto::pb::ServerMessage>(64);
    registry.register(&agent_id, msg_tx).await;

    let job = JobRepo::new(db.clone())
        .create(host_id, "sleep", &["999".to_string()])
        .await
        .expect("create");
    JobRepo::new(db.clone())
        .set_status(job.id, "running")
        .await
        .expect("running");

    let service = ExecService::new(db.clone(), registry);
    let (cancelled, delivered, compensated) = service.cancel(job.id).await.expect("cancel");
    assert!(cancelled);
    assert!(delivered, "online agent must receive JobCancel");
    assert!(!compensated);

    let msg = tokio::time::timeout(std::time::Duration::from_secs(5), msg_rx.recv())
        .await
        .expect("timed out")
        .expect("channel open");
    match msg.kind {
        Some(server_message::Kind::JobCancel(c)) => {
            assert_eq!(c.job_id, job.id.to_string());
        }
        other => panic!("expected JobCancel, got {other:?}"),
    }

    let row = get_job_raw(&db, job.id).await;
    assert_eq!(row.status, "cancelled");
}

#[tokio::test]
async fn cancel_running_offline_compensates_for_reconnect() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new(); // 不注册连接 = agent 离线
    let (agent_id, host_id) = seed_agent(&db).await;

    let job = JobRepo::new(db.clone())
        .create(host_id, "sleep", &["999".to_string()])
        .await
        .expect("create");
    JobRepo::new(db.clone())
        .set_status(job.id, "running")
        .await
        .expect("running");

    let service = ExecService::new(db.clone(), registry);
    let (cancelled, delivered, compensated) = service.cancel(job.id).await.expect("cancel");
    assert!(cancelled);
    assert!(!delivered, "offline agent cannot receive cancel now");
    assert!(compensated, "offline cancel must record compensation");

    let row = get_job_raw(&db, job.id).await;
    assert_eq!(row.status, "cancelled");

    // 补偿在账：重连瞬间 take_pending_cancels 应取出该 job（然后由连接层补发 JobCancel）
    let pending = JobRepo::new(db.clone())
        .take_pending_cancels(&agent_id)
        .await
        .expect("take");
    assert_eq!(pending, vec![job.id]);
    // 再取为空（取即清除）
    let again = JobRepo::new(db.clone())
        .take_pending_cancels(&agent_id)
        .await
        .expect("take");
    assert!(again.is_empty());
}

#[tokio::test]
async fn cancel_terminal_job_is_rejected() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let (_agent_id, host_id) = seed_agent(&db).await;

    let job = JobRepo::new(db.clone())
        .create(host_id, "true", &[])
        .await
        .expect("create");
    JobRepo::new(db.clone())
        .set_status(job.id, "succeeded")
        .await
        .expect("succeeded");

    let service = ExecService::new(db.clone(), registry);
    let err = service
        .cancel(job.id)
        .await
        .expect_err("terminal job must reject");
    assert_eq!(err.code(), "invalid_argument");
}
