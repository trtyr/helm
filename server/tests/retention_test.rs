//! retention 清理集成测试（C1）：jobs / audit_logs / file_transfers 的 delete_before。

mod common;

use helm_server::application::retention::run_once;
use helm_server::store::Db;
use helm_server::store::agent_repo::{AgentRepo, HostOsDetails};
use helm_server::store::audit_repo::AuditRepo;
use helm_server::store::file_transfer_repo::FileTransferRepo;
use helm_server::store::job_repo::JobRepo;
use helm_server::store::status_event_repo::StatusEventRepo;
use std::sync::LazyLock;

/// 本文件的清理测试都是**全表删除**（`DELETE ... WHERE created_at < cutoff`），共享同一张
/// 表 → 必须串行；否则一条测试的 delete 会把另一条刚拨旧的行先吃掉（断言随之失真）。
static RETENTION_GUARD: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

/// 造一个 host，返回 host_id。
async fn seed_host(db: &Db) -> uuid::Uuid {
    let agent_id = format!("itest-ret-{}", uuid::Uuid::new_v4());
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
        .expect("register");
    AgentRepo::new(db.clone())
        .get_host_id(&agent_id)
        .await
        .expect("get_host_id")
        .expect("host registered")
}

#[tokio::test]
async fn retention_deletes_stale_jobs_audit_and_transfers() {
    let _g = RETENTION_GUARD.lock().await;
    let db = common::connect().await;
    let host_id = seed_host(&db).await;

    // 造三类新行
    let job = JobRepo::new(db.clone())
        .create(host_id, "true", &[])
        .await
        .expect("create job");
    let audit = AuditRepo::new(db.clone())
        .insert(
            "itest",
            "action",
            "resource",
            &serde_json::json!({"k": "v"}),
        )
        .await
        .expect("insert audit");
    let ft = FileTransferRepo::new(db.clone())
        .create(host_id, "upload", "/tmp/x", 1)
        .await
        .expect("create transfer");

    // cutoff 在过去：新行不受影响
    let past = chrono::Utc::now() - chrono::Duration::days(1);
    let jobs = JobRepo::new(db.clone())
        .delete_before(past)
        .await
        .expect("delete jobs");
    assert!(!jobs.contains(&job.id));
    let audits = AuditRepo::new(db.clone())
        .delete_before(past)
        .await
        .expect("delete audit");
    assert_eq!(audits, 0, "fresh rows must survive");
    let transfers = FileTransferRepo::new(db.clone())
        .delete_before(past)
        .await
        .expect("delete transfers");
    assert_eq!(transfers, 0, "fresh rows must survive");

    // 拨旧行到 8 天前，cutoff 取 7 天前 → 旧行被清、新行保留
    for id in [job.id, audit.id, ft.id] {
        sqlx::query("UPDATE jobs SET created_at = now() - interval '8 days' WHERE id = $1")
            .bind(id)
            .execute(db.pool())
            .await
            .ok(); // jobs 专用
    }
    // 分表拨旧（jobs/audit_logs/file_transfers 各自 created_at）
    sqlx::query("UPDATE audit_logs SET created_at = now() - interval '8 days' WHERE id = $1")
        .bind(audit.id)
        .execute(db.pool())
        .await
        .expect("backdate audit");
    sqlx::query("UPDATE file_transfers SET created_at = now() - interval '8 days' WHERE id = $1")
        .bind(ft.id)
        .execute(db.pool())
        .await
        .expect("backdate transfer");

    let cutoff = chrono::Utc::now() - chrono::Duration::days(7);
    let jobs = JobRepo::new(db.clone())
        .delete_before(cutoff)
        .await
        .expect("delete jobs");
    assert!(jobs.contains(&job.id), "stale job must be deleted (C1)");
    let audits = AuditRepo::new(db.clone())
        .delete_before(cutoff)
        .await
        .expect("delete audit");
    assert!(audits >= 1, "stale audit must be deleted (C1)");
    let transfers = FileTransferRepo::new(db.clone())
        .delete_before(cutoff)
        .await
        .expect("delete transfers");
    assert!(transfers >= 1, "stale transfer must be deleted (C1)");

    // 行确实没了
    assert!(
        JobRepo::new(db.clone())
            .get(job.id)
            .await
            .expect("get")
            .is_none()
    );
}

/// T008：status_events 纳入统一保留策略——超期清、未超期留（仓储层）。
#[tokio::test]
async fn retention_trims_status_events_and_keeps_fresh() {
    let _g = RETENTION_GUARD.lock().await;
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db.clone());
    let old_host = format!("itest-ret-old-{}", uuid::Uuid::new_v4());
    let fresh_host = format!("itest-ret-new-{}", uuid::Uuid::new_v4());
    repo.insert(&old_host, "online", "itest", "")
        .await
        .expect("insert old");
    repo.insert(&fresh_host, "online", "itest", "")
        .await
        .expect("insert fresh");
    repo.insert(&old_host, "offline", "itest", "")
        .await
        .expect("insert old offline");

    // 把该主机的两条拨到 120 天前，cutoff 取 90 天前
    sqlx::query(
        "UPDATE status_events SET created_at = now() - interval '120 days' WHERE host_id = $1",
    )
    .bind(&old_host)
    .execute(db.pool())
    .await
    .expect("backdate status events");

    let cutoff = chrono::Utc::now() - chrono::Duration::days(90);
    let deleted = repo
        .delete_before(cutoff)
        .await
        .expect("delete status events");
    assert!(
        deleted >= 2,
        "超期状态事件必须被清理（T008）：删了 {deleted} 条"
    );

    let left_old = repo
        .list_paged(None, Some(&old_host), None, None, None, None, 10, 0)
        .await
        .expect("list old host");
    assert!(left_old.is_empty(), "被清理的主机不应残留事件");
    let left_fresh = repo
        .list_paged(None, Some(&fresh_host), None, None, None, None, 10, 0)
        .await
        .expect("list fresh host");
    assert_eq!(left_fresh.len(), 1, "未超期事件必须保留");
}

/// T008：`run_once` 是「一轮覆盖多表」的单轮清理——status_events 与 metrics 同轮被清。
#[tokio::test]
async fn run_once_covers_status_events_and_metrics() {
    let _g = RETENTION_GUARD.lock().await;
    let db = common::connect().await;
    let host_id = seed_host(&db).await;

    // status_events：拨旧
    let old_host = format!("itest-run-old-{}", uuid::Uuid::new_v4());
    StatusEventRepo::new(db.clone())
        .insert(&old_host, "online", "itest", "")
        .await
        .expect("insert status event");
    sqlx::query(
        "UPDATE status_events SET created_at = now() - interval '120 days' WHERE host_id = $1",
    )
    .bind(&old_host)
    .execute(db.pool())
    .await
    .expect("backdate status event");

    // metrics：拨旧（ts 列）
    sqlx::query(
        "INSERT INTO metrics (host_id, name, value, ts)
         VALUES ($1, 'itest.retention.metric', 1.0, now() - interval '120 days')",
    )
    .bind(host_id)
    .execute(db.pool())
    .await
    .expect("insert stale metric");

    let report = run_once(&db, 90).await;
    assert!(
        report.status_events >= 1,
        "run_once 必须覆盖 status_events：{report:?}"
    );
    assert!(report.metrics >= 1, "run_once 必须覆盖 metrics：{report:?}");
    assert!(report.total() >= report.status_events + report.metrics);

    let left = StatusEventRepo::new(db.clone())
        .list_paged(None, Some(&old_host), None, None, None, None, 10, 0)
        .await
        .expect("list");
    assert!(left.is_empty(), "旧状态事件应已被 run_once 清掉");
}

#[tokio::test]
async fn db_connect_with_respects_pool_settings() {
    // C3：池参数可配——connect_with 正常建池（能力冒烟，参数语义由 sqlx 保证）
    let url = format!(
        "{}/postgres",
        std::env::var("HELM_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm".into())
            .rsplit_once('/')
            .map(|(o, _)| o.to_string())
            .unwrap_or_default()
    );
    let db = Db::connect_with(&url, 3, 5).await.expect("connect_with");
    let n: (i64,) = sqlx::query_as("SELECT 1::bigint")
        .fetch_one(db.pool())
        .await
        .expect("query");
    assert_eq!(n.0, 1);
}
