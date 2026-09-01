//! 仓储层集成测试：连真实 Postgres 验证迁移 + host_repo 读写。
//! 需 `HELM_DATABASE_URL`（默认 docker compose 的 5433）与已启动的 Postgres。

use helm_server::store::Db;
use helm_server::store::host_repo::{HostRepo, NewHost};

fn test_url() -> String {
    std::env::var("HELM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm".to_string())
}

#[tokio::test]
async fn host_repo_insert_list_soft_delete() {
    let db = Db::connect(&test_url()).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repo = HostRepo::new(db);

    let host = NewHost {
        hostname: "itest-host".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        platform: "linux-x86_64".into(),
        tags: vec!["itest".into()],
        conn_mode: "reverse".into(),
        addr: String::new(),
    };

    let row = repo.insert(&host).await.expect("insert");
    assert_eq!(row.hostname, "itest-host");

    let rows = repo.list().await.expect("list");
    assert!(rows.iter().any(|r| r.hostname == "itest-host"));

    let n = repo.soft_delete(row.id).await.expect("soft_delete");
    assert_eq!(n, 1);

    let rows = repo.list().await.expect("list after delete");
    assert!(!rows.iter().any(|r| r.hostname == "itest-host"));
}
