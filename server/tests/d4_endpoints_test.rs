//! D4 三端点集成测试：GET /hosts/{id}、GET /jobs?status=&host_id=、services 状态广播。

mod common;

use helm_server::store::Db;
use helm_server::store::agent_repo::{AgentRepo, HostOsDetails};
use helm_server::store::job_repo::JobRepo;
use helm_server::store::service_repo::ServiceRepo;

/// 造一个 host，返回 host_id。
async fn seed_host(db: &Db) -> uuid::Uuid {
    let agent_id = format!("itest-d4-{}", uuid::Uuid::new_v4());
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
async fn host_get_returns_seeded_host() {
    // GET /hosts/{id} 语义：HostRepo::get 命中 / 未命中 None
    let db = common::connect().await;
    let host_id = seed_host(&db).await;
    let found = helm_server::store::host_repo::HostRepo::new(db.clone())
        .get(host_id)
        .await
        .expect("get");
    assert!(found.is_some(), "seeded host must be found by id");
    let missing = helm_server::store::host_repo::HostRepo::new(db.clone())
        .get(uuid::Uuid::new_v4())
        .await
        .expect("get");
    assert!(missing.is_none(), "unknown id must 404");
}

#[tokio::test]
async fn jobs_list_filters_by_status_and_host() {
    // GET /jobs?status=&host_id= 语义：list_filtered 三种组合
    let db = common::connect().await;
    let host_a = seed_host(&db).await;
    let host_b = seed_host(&db).await;

    let ja = JobRepo::new(db.clone())
        .create(host_a, "true", &[])
        .await
        .expect("create a");
    let jb = JobRepo::new(db.clone())
        .create(host_b, "true", &[])
        .await
        .expect("create b");
    JobRepo::new(db.clone())
        .set_status(ja.id, "running")
        .await
        .expect("running");

    // status 过滤
    let running = JobRepo::new(db.clone())
        .list_filtered(Some("running"), None, None, None, 100, 0, None)
        .await
        .expect("filter status");
    assert!(running.iter().any(|j| j.id == ja.id));
    assert!(
        !running.iter().any(|j| j.id == jb.id),
        "queued job must not match running filter"
    );

    // host_id 过滤
    let of_b = JobRepo::new(db.clone())
        .list_filtered(None, Some(host_b), None, None, 100, 0, None)
        .await
        .expect("filter host");
    assert!(of_b.iter().all(|j| j.host_id == host_b));
    assert!(of_b.iter().any(|j| j.id == jb.id));

    // 组合过滤
    let both = JobRepo::new(db.clone())
        .list_filtered(Some("queued"), Some(host_b), None, None, 100, 0, None)
        .await
        .expect("filter both");
    assert!(both.iter().any(|j| j.id == jb.id));
    assert!(
        both.iter()
            .all(|j| j.host_id == host_b && j.status == "queued")
    );

    // 无过滤 = 全量（两台都有）
    let all = JobRepo::new(db.clone())
        .list_filtered(None, None, None, None, 100, 0, None)
        .await
        .expect("no filter");
    assert!(all.iter().any(|j| j.id == ja.id) && all.iter().any(|j| j.id == jb.id));
}

#[tokio::test]
async fn job_time_range_filter_mirrors_count() {
    // P003 T7：from/to 时间范围过滤 + count/list 同范围镜像断言
    let db = common::connect().await;
    let host = seed_host(&db).await;
    let repo = JobRepo::new(db.clone());

    let j = repo.create(host, "true", &[]).await.expect("create");
    // created_at 回填 2 小时前
    sqlx::query("UPDATE jobs SET created_at = now() - interval '2 hours' WHERE id = $1")
        .bind(j.id)
        .execute(db.pool())
        .await
        .expect("backdate");

    let hour_ago = chrono::Utc::now() - chrono::Duration::hours(1);
    let three_hours_ago = chrono::Utc::now() - chrono::Duration::hours(3);

    // from = 1 小时前：2 小时前的 job 不命中（边界正确）
    let list_after = repo
        .list_filtered(Some("queued"), None, Some(hour_ago), None, 100, 0, None)
        .await
        .expect("list from");
    let count_after = repo
        .count_filtered(Some("queued"), None, Some(hour_ago), None)
        .await
        .expect("count from");
    assert_eq!(
        list_after.len() as i64,
        count_after,
        "count/list 同范围镜像（status+from）"
    );
    assert!(
        !list_after.iter().any(|r| r.id == j.id),
        "2 小时前的 job 不应命中 from=1 小时前"
    );

    // from = 3 小时前：命中 + 镜像
    let list_in = repo
        .list_filtered(None, Some(host), Some(three_hours_ago), None, 100, 0, None)
        .await
        .expect("list from wide");
    let count_in = repo
        .count_filtered(None, Some(host), Some(three_hours_ago), None)
        .await
        .expect("count from wide");
    assert_eq!(
        list_in.len() as i64,
        count_in,
        "count/list 同范围镜像（host+from）"
    );
    assert!(list_in.iter().any(|r| r.id == j.id), "3 小时前的窗口应命中");
}

#[tokio::test]
async fn service_status_changes_broadcast_on_services_topic() {
    // D4 services 流：ServiceStatus 入站广播（用 StreamRegistry 直接验证 topic 语义）
    let db = common::connect().await;
    let streams = helm_server::grpc::stream_registry::StreamRegistry::new();
    let mut rx = streams.subscribe("services").await;

    streams
        .broadcast(
            "services",
            serde_json::json!({
                "service_id": uuid::Uuid::new_v4(),
                "status": "running",
                "pid": 4321,
                "exit_code": null,
            })
            .to_string()
            .into_bytes(),
        )
        .await;

    let got = tokio::time::timeout(std::time::Duration::from_secs(2), rx.recv())
        .await
        .expect("broadcast within 2s")
        .expect("message");
    let payload: serde_json::Value = serde_json::from_slice(&got).expect("json");
    assert_eq!(payload["status"], "running");
    assert_eq!(payload["pid"], 4321);

    // 快照来源：ServiceRepo::list 可用（services_stream 连接首推的数据源）
    let repo = ServiceRepo::new(db.clone());
    let list = repo.list().await.expect("list");
    let _ = list; // 空表也合法——快照可为空数组
}
