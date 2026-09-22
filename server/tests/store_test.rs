//! 仓储层集成测试：连专用临时库（helm_itest，见 tests/common） 验证迁移 + host_repo 读写。
//! 需 Postgres（docker compose 5433），测试库自动重建。

mod common;

use helm_server::store::alert_repo::AlertRepo;
use helm_server::store::audit_repo::AuditRepo;
use helm_server::store::host_repo::{HostRepo, NewHost};

#[tokio::test]
async fn host_repo_insert_list_soft_delete() {
    let db = common::connect().await;
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
    let db = common::connect().await;
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

#[tokio::test]
async fn host_repo_get_update_paged() {
    let db = common::connect().await;
    let repo = HostRepo::new(db);

    let hostname = format!("itest-upd-{}", std::process::id());
    let row = repo
        .insert(&NewHost {
            hostname: hostname.clone(),
            os: "linux".into(),
            arch: "x86_64".into(),
            platform: "linux-x86_64".into(),
            tags: vec!["a".into()],
            conn_mode: "reverse".into(),
            addr: String::new(),
        })
        .await
        .expect("insert");

    let got = repo.get(row.id).await.expect("get").expect("row");
    assert_eq!(got.hostname, hostname);

    let updated = repo
        .update(
            row.id,
            &NewHost {
                hostname: "renamed".into(),
                os: "linux".into(),
                arch: "x86_64".into(),
                platform: "linux-x86_64".into(),
                tags: vec!["b".into()],
                conn_mode: "forward".into(),
                addr: "1.2.3.4:50052".into(),
            },
        )
        .await
        .expect("update")
        .expect("row");
    assert_eq!(updated.hostname, "renamed");
    assert_eq!(updated.tags, vec!["b".to_string()]);
    assert_eq!(updated.conn_mode, "forward");

    let paged = repo.list_paged(1, 0).await.expect("list_paged");
    assert_eq!(paged.len(), 1);

    let _ = repo.soft_delete(row.id).await;
}

#[tokio::test]
async fn audit_repo_insert_list() {
    let db = common::connect().await;
    let repo = AuditRepo::new(db);

    let row = repo
        .insert(
            "admin",
            "exec",
            "agent-x",
            &serde_json::json!({ "command": "ls" }),
        )
        .await
        .expect("insert");
    assert_eq!(row.actor, "admin");
    assert_eq!(row.action, "exec");
    assert_eq!(row.resource, "agent-x");

    let rows = repo.list(10).await.expect("list");
    assert!(rows.iter().any(|r| r.id == row.id));
}

/// P006 P0-3：审计关键词搜索必须可用。
///
/// `audit_logs.detail` 是 **JSONB**，而 PG 没有 `jsonb ~~* text` 运算符——旧实现
/// （`detail ILIKE $1`）会让**任何带 `q=` 的请求**在运行期报
/// `operator does not exist: jsonb ~~* character varying` → HTTP 500。
#[tokio::test]
async fn audit_repo_keyword_search_does_not_error() {
    let db = common::connect().await;
    let repo = AuditRepo::new(db);

    let marker = format!("p006-marker-{}", std::process::id());
    repo.insert(
        &marker,
        "login",
        "res-p006",
        &serde_json::json!({ "note": marker }),
    )
    .await
    .expect("insert");

    let rows = repo
        .list_paged(10, 0, None, Some(&marker), None, None, None)
        .await
        .expect("关键词搜索不得报错");
    assert!(
        rows.iter().any(|r| r.actor == marker),
        "应能命中 actor；结果 {} 条",
        rows.len()
    );

    // `detail`（JSONB）也要能被搜到——这一条正是旧实现直接 500 的路径
    let only_in_detail = format!("p006-detail-{}", std::process::id());
    repo.insert(
        "someone",
        "login",
        "r",
        &serde_json::json!({ "k": only_in_detail }),
    )
    .await
    .expect("insert");
    let rows = repo
        .list_paged(10, 0, None, Some(&only_in_detail), None, None, None)
        .await
        .expect("detail(JSONB) 关键词搜索不得报错");
    assert!(!rows.is_empty(), "detail 里的关键词应命中");

    // LIKE 元字符与结尾反斜杠：未转义会改写匹配语义，或直接
    // 报 `LIKE pattern must not end with escape character`
    for kw in ["%", "_", "\\", "100%", "a_b"] {
        repo.list_paged(10, 0, None, Some(kw), None, None, None)
            .await
            .unwrap_or_else(|e| panic!("关键词 {kw:?} 搜索报错: {e}"));
    }
}

#[tokio::test]
async fn alert_repo_insert_list_cleanup() {
    let db = common::connect().await;
    let host = HostRepo::new(db.clone())
        .insert(&NewHost {
            hostname: format!("itest-alert-{}", std::process::id()),
            os: String::new(),
            arch: String::new(),
            platform: String::new(),
            tags: vec![],
            conn_mode: "reverse".into(),
            addr: String::new(),
        })
        .await
        .expect("host");
    let repo = AlertRepo::new(db);

    let row = repo
        .insert(host.id, "cpu.usage", 90.0, 95.0)
        .await
        .expect("insert");
    assert_eq!(row.metric_name, "cpu.usage");

    let rows = repo.list(10).await.expect("list");
    assert!(rows.iter().any(|r| r.id == row.id));

    // 时序保留：删除未来时间之前（即全部）应删 1 条
    let n = repo
        .delete_before(chrono::Utc::now() + chrono::Duration::hours(1))
        .await
        .expect("delete");
    assert_eq!(n, 1);
}
