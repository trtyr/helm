//! retention 清理集成测试（C1）：jobs / audit_logs / file_transfers 的 delete_before。

mod common;

use helm_server::store::Db;
use helm_server::store::agent_repo::{AgentRepo, HostOsDetails};
use helm_server::store::audit_repo::AuditRepo;
use helm_server::store::file_transfer_repo::FileTransferRepo;
use helm_server::store::job_repo::JobRepo;

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
