//! exec_service 集成测试：连真实 Postgres 验证编排失败分支。

use helm_server::application::exec_service::ExecService;
use helm_server::domain::Error;
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::store::Db;

fn test_url() -> String {
    std::env::var("HELM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm".to_string())
}

#[tokio::test]
async fn exec_unknown_agent_returns_not_found() {
    let db = Db::connect(&test_url()).await.expect("connect");
    db.migrate().await.expect("migrate");
    let registry = ConnectionRegistry::new();
    let service = ExecService::new(db, registry);

    let err = service
        .exec("no-such-agent", "echo", &[])
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
    assert_eq!(err.code(), "not_found");
    assert!(!err.retryable());
}
