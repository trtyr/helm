//! 单用户账号管理集成测试：连专用临时库（helm_itest，见 tests/common） 验证 /auth/me 数据源、
//! 改密（旧密拒绝/新密生效/最短长度）、改用户名（查重冲突/生效后旧名消失）。
//! 需 Postgres（docker compose 5433），测试库自动重建。

mod common;

use helm_server::application::auth_service::AuthService;
use helm_server::domain::Error;
use helm_server::store::Db;
use helm_server::store::user_repo::UserRepo;


fn hash(plain: &str) -> String {
    bcrypt::hash(plain, 4).expect("bcrypt hash")
}

/// 清理指定前缀的测试用户（测试间并行，前缀必须互不重叠）。
async fn cleanup(db: &Db, prefix: &str) {
    sqlx::query("DELETE FROM users WHERE username LIKE $1")
        .bind(format!("{prefix}%"))
        .execute(db.pool())
        .await
        .expect("cleanup");
}

/// 建一个测试用户。
async fn setup_user(db: &Db, username: &str, password: &str) {
    UserRepo::new(db.clone())
        .create(username, &hash(password), "admin")
        .await
        .expect("create user");
}

#[tokio::test]
async fn account_view_reflects_db_and_stale_sub_unauthorized() {
    let db = common::connect().await;
    let prefix = format!("itest-acct{}-me", std::process::id());
    let name = format!("{prefix}-u");
    cleanup(&db, &prefix).await;
    setup_user(&db, &name, "password-1").await;
    let auth = AuthService::new(db.clone(), "secret".into());

    let view = auth.account(&name).await.expect("account");
    assert_eq!(view.username, name);
    assert_eq!(view.role, "admin");

    // sub 不存在（如改名后旧 token）→ 401
    let err = auth.account(&format!("{name}-gone")).await.unwrap_err();
    assert!(matches!(err, Error::Unauthorized(_)));

    cleanup(&db, &prefix).await;
}

#[tokio::test]
async fn change_password_rejects_old_and_accepts_new() {
    let db = common::connect().await;
    let prefix = format!("itest-acct{}-pw", std::process::id());
    let name = format!("{prefix}-u");
    cleanup(&db, &prefix).await;
    setup_user(&db, &name, "old-pass-1").await;
    let auth = AuthService::new(db.clone(), "secret".into());

    // 当前密码错 → 401
    let err = auth
        .change_password(&name, "wrong-pass", "new-pass-1")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Unauthorized(_)), "{err:?}");

    // 新密码过短 → 400
    let err = auth
        .change_password(&name, "old-pass-1", "12345")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");

    // 正确流程 → 新密可登录、旧密失效
    auth.change_password(&name, "old-pass-1", "new-pass-1")
        .await
        .expect("change password");
    assert!(auth.login(&name, "new-pass-1").await.unwrap().is_some());
    assert!(auth.login(&name, "old-pass-1").await.unwrap().is_none());

    cleanup(&db, &prefix).await;
}

#[tokio::test]
async fn change_username_conflict_and_success() {
    let db = common::connect().await;
    let prefix = format!("itest-acct{}-un", std::process::id());
    let name_a = format!("{prefix}-a");
    let name_b = format!("{prefix}-b");
    cleanup(&db, &prefix).await;
    setup_user(&db, &name_a, "password-1").await;
    setup_user(&db, &name_b, "password-2").await;
    let auth = AuthService::new(db.clone(), "secret".into());

    // 当前密码错 → 401
    let err = auth
        .change_username(&name_a, "wrong-pass", "brand-new")
        .await
        .unwrap_err();
    assert!(matches!(err, Error::Unauthorized(_)), "{err:?}");

    // 目标名已存在 → 400
    let err = auth
        .change_username(&name_a, "password-1", &name_b)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::InvalidArgument(_)), "{err:?}");

    // 成功改名 → 旧名查无、新名可登录（密码不变）
    let new_name = format!("{prefix}-renamed");
    auth.change_username(&name_a, "password-1", &new_name)
        .await
        .expect("rename");
    assert!(
        UserRepo::new(db.clone())
            .get_by_username(&name_a)
            .await
            .unwrap()
            .is_none()
    );
    let renamed = UserRepo::new(db.clone())
        .get_by_username(&new_name)
        .await
        .unwrap()
        .expect("renamed exists");
    assert_eq!(renamed.username, new_name);
    assert!(auth.login(&new_name, "password-1").await.unwrap().is_some());

    // 改名后旧 sub 失效 → account 返回 401
    let err = auth.account(&name_a).await.unwrap_err();
    assert!(matches!(err, Error::Unauthorized(_)));

    cleanup(&db, &prefix).await;
}
