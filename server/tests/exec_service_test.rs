//! exec_service 集成测试：连专用临时库（helm_itest，见 tests/common） 验证编排失败分支。

mod common;

use helm_server::application::exec_service::ExecService;
use helm_server::domain::Error;
use helm_server::grpc::connection_registry::ConnectionRegistry;

#[tokio::test]
async fn exec_unknown_agent_returns_not_found() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let service = ExecService::new(db, registry);

    let err = service
        .exec("no-such-agent", "echo", &[], None)
        .await
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)));
    assert_eq!(err.code(), "not_found");
    assert!(!err.retryable());
}
