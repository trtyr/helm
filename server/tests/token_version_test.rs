//! A5 集成测试：JWT 吊销（`users.token_version`）。
//!
//! 复现的攻击路径：JWT 24h 有效且无吊销机制 —— 改密/改名后，**已被泄露的旧 token 在剩余
//! 有效期内仍可访问非账号类端点**。修复后：改密/改名自增版本号，中间件逐请求核对，
//! 旧 token 立即 401。
//!
//! 需 Postgres（docker compose 5433），测试库自动重建。

mod common;

use helm_server::application::auth_service::AuthService;

/// 唯一用户名（避免与其他测试的共享库互相干扰）。
fn unique(prefix: &str) -> String {
    format!("{prefix}-{}", uuid::Uuid::new_v4())
}

#[tokio::test]
async fn password_change_revokes_existing_jwt() {
    let db = common::connect().await;
    let user = unique("tv-test");
    let auth = AuthService::new(db.clone(), "test-secret".into());

    let repo = helm_server::store::user_repo::UserRepo::new(db.clone());
    let hash = bcrypt::hash("old-password", bcrypt::DEFAULT_COST).unwrap();
    let row = repo.create(&user, &hash, "admin").await.unwrap();
    assert_eq!(row.token_version, 1, "迁移默认值为 1");

    // 登录拿 token（tv=1）
    let token = auth
        .login(&user, "old-password")
        .await
        .unwrap()
        .expect("login ok");
    assert!(
        auth.verify_revocable(&token).await.is_ok(),
        "改密前 token 有效"
    );

    // 改密 → 版本号自增 → 旧 token 立即失效
    auth.change_password(&user, "old-password", "new-password")
        .await
        .unwrap();
    let err = auth.verify_revocable(&token).await;
    assert!(err.is_err(), "改密后旧 token 必须被吊销");

    // 新登录的 token 有效（tv 已跟上）
    let fresh = auth
        .login(&user, "new-password")
        .await
        .unwrap()
        .expect("re-login ok");
    assert!(
        auth.verify_revocable(&fresh).await.is_ok(),
        "重新登录后的 token 有效"
    );
}

#[tokio::test]
async fn username_change_revokes_existing_jwt() {
    let db = common::connect().await;
    let user = unique("tv-rename");
    let new_name = unique("tv-renamed");
    let auth = AuthService::new(db.clone(), "test-secret".into());

    let repo = helm_server::store::user_repo::UserRepo::new(db.clone());
    let hash = bcrypt::hash("pw-123456", bcrypt::DEFAULT_COST).unwrap();
    repo.create(&user, &hash, "admin").await.unwrap();

    let token = auth.login(&user, "pw-123456").await.unwrap().unwrap();
    assert!(auth.verify_revocable(&token).await.is_ok());

    auth.change_username(&user, "pw-123456", &new_name)
        .await
        .unwrap();
    // 旧 token 的 sub 已不存在 + tv 落后 —— 两种失效路径任一即拒
    assert!(
        auth.verify_revocable(&token).await.is_err(),
        "改名后旧 token 必须被吊销"
    );
}
