//! API key 集成测试：连专用临时库（helm_itest，见 tests/common） 验证创建/校验/吊销/过期全链路
//! （repo 读写 + service 哈希往返 + 冷 key 拒绝）。需 Postgres（docker compose 5433）。
//! （默认 docker compose 的 5433）与已启动的 Postgres。

mod common;

use helm_server::application::api_key_service::ApiKeyService;
use helm_server::store::Db;
use std::time::Duration;

/// 清理本测试创建的 key（按名称前缀）。
async fn cleanup(db: &Db, name_prefix: &str) {
    sqlx::query("DELETE FROM api_keys WHERE name LIKE $1")
        .bind(format!("{name_prefix}%"))
        .execute(db.pool())
        .await
        .expect("cleanup api keys");
}

#[tokio::test]
async fn create_verify_roundtrip() {
    let db = common::connect().await;
    let name = format!("itest-key-{}", std::process::id());
    cleanup(&db, &name).await;

    let svc = ApiKeyService::new(db.clone());
    let (row, raw) = svc.create(&name, None, Vec::new()).await.expect("create");

    // 明文格式与落库形态
    assert!(raw.starts_with("helm_"));
    assert_ne!(raw, row.key_hash, "db stores hash, not raw");
    assert_eq!(row.prefix.len(), 12);
    assert!(row.prefix.starts_with("helm_"));

    // 正确 key → 命中并返回（key 名, scopes）
    let (verified_name, scopes) = svc.verify(&raw).await.expect("verify ok").expect("hit");
    assert_eq!(verified_name, name);
    assert!(scopes.is_empty(), "默认创建 = 空 scopes = 全功能");

    // 错误 key / 乱前缀 → 拒绝
    assert_eq!(svc.verify("helm_deadbeef").await.unwrap(), None);
    assert_eq!(svc.verify("not-even-a-key").await.unwrap(), None);

    cleanup(&db, &name).await;
}

#[tokio::test]
async fn revoked_key_rejected_and_idempotent() {
    let db = common::connect().await;
    let name = format!("itest-revoke-{}", std::process::id());
    cleanup(&db, &name).await;

    let svc = ApiKeyService::new(db.clone());
    let (row, raw) = svc.create(&name, None, Vec::new()).await.expect("create");
    assert!(svc.verify(&raw).await.unwrap().is_some());

    // 吊销后立即失效
    svc.revoke(row.id).await.expect("revoke");
    assert_eq!(svc.verify(&raw).await.unwrap(), None);

    // 幂等：重复吊销不报错
    svc.revoke(row.id).await.expect("revoke again");

    // 管理查询仍可见（含已吊销）
    let got = svc.get(row.id).await.expect("get").expect("row exists");
    assert!(got.revoked_at.is_some());

    cleanup(&db, &name).await;
}

#[tokio::test]
async fn expired_key_rejected() {
    let db = common::connect().await;
    let name = format!("itest-expired-{}", std::process::id());
    cleanup(&db, &name).await;

    let svc = ApiKeyService::new(db.clone());
    let expired = chrono::Utc::now() - chrono::Duration::seconds(1);
    let (_row, raw) = svc
        .create(&name, Some(expired), Vec::new())
        .await
        .expect("create");

    assert_eq!(svc.verify(&raw).await.unwrap(), None, "expired must fail");

    cleanup(&db, &name).await;
}

#[tokio::test]
async fn scoped_key_roundtrip_and_validation() {
    let db = common::connect().await;
    let name = format!("itest-scoped-{}", std::process::id());
    cleanup(&db, &name).await;

    let svc = ApiKeyService::new(db.clone());
    // 限定 scope 的 key：verify 带回 scopes
    let (_row, raw) = svc
        .create(&name, None, vec!["exec".into(), "metrics".into()])
        .await
        .expect("create scoped");
    let (_, scopes) = svc.verify(&raw).await.unwrap().expect("hit");
    assert_eq!(scopes, vec!["exec".to_string(), "metrics".to_string()]);

    // 未知 scope → 创建被拒绝
    let err = svc
        .create(&format!("{name}-bad"), None, vec!["nope".into()])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("nope"), "err: {err}");

    cleanup(&db, &name).await;
}

#[tokio::test]
async fn list_paged_and_touch_last_used() {
    let db = common::connect().await;
    let name = format!("itest-list-{}", std::process::id());
    cleanup(&db, &name).await;

    let svc = ApiKeyService::new(db.clone());
    let (_r1, raw1) = svc
        .create(&format!("{name}-a"), None, Vec::new())
        .await
        .expect("create a");
    svc.create(&format!("{name}-b"), None, Vec::new())
        .await
        .expect("create b");

    // verify 触发 last_used_at 刷新
    svc.verify(&raw1).await.expect("verify");
    tokio::time::sleep(Duration::from_millis(20)).await;

    let rows = svc.list_paged(100, 0).await.expect("list");
    let mine: Vec<_> = rows.iter().filter(|r| r.name.starts_with(&name)).collect();
    assert_eq!(mine.len(), 2);
    let a = mine.iter().find(|r| r.name.ends_with("-a")).unwrap();
    assert!(a.last_used_at.is_some(), "touch should set last_used_at");
    let b = mine.iter().find(|r| r.name.ends_with("-b")).unwrap();
    assert!(b.last_used_at.is_none());

    cleanup(&db, &name).await;
}
