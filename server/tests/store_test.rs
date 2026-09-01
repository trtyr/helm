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

#[tokio::test]
async fn host_repo_list_by_tag_and_set_tags() {
    let db = Db::connect(&test_url()).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repo = HostRepo::new(db);

    let hostname = format!("itest-tags-{}", std::process::id());
    let host = NewHost {
        hostname: hostname.clone(),
        os: String::new(),
        arch: String::new(),
        platform: String::new(),
        tags: vec!["group-a".into()],
        conn_mode: "reverse".into(),
        addr: String::new(),
    };
    let row = repo.insert(&host).await.expect("insert");

    // 按标签过滤命中
    let by_tag = repo.list_by_tag("group-a").await.expect("list_by_tag");
    assert!(by_tag.iter().any(|r| r.id == row.id));
    // 不存在的标签不含该行
    let empty = repo
        .list_by_tag("nonexistent-tag")
        .await
        .expect("list_by_tag");
    assert!(empty.iter().all(|r| r.id != row.id));

    // 覆盖设置标签
    let updated = repo
        .set_tags(row.id, &["new-group".into()])
        .await
        .expect("set_tags")
        .expect("row exists");
    assert_eq!(updated.tags, vec!["new-group".to_string()]);
    let old = repo.list_by_tag("group-a").await.expect("list_by_tag");
    assert!(old.iter().all(|r| r.id != row.id));
    let new = repo.list_by_tag("new-group").await.expect("list_by_tag");
    assert!(new.iter().any(|r| r.id == row.id));

    // 未找到的主机返回 None
    let missing = repo
        .set_tags(uuid::Uuid::new_v4(), &["x".into()])
        .await
        .expect("set_tags");
    assert!(missing.is_none());

    let _ = repo.soft_delete(row.id).await;
}
