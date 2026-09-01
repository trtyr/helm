//! Agent 生命周期集成测试：注销删除 agent + 孤儿 host。

use helm_server::application::agent_lifecycle_service::AgentLifecycleService;
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::store::Db;
use helm_server::store::agent_repo::AgentRepo;
use helm_server::store::host_repo::HostRepo;

fn test_url() -> String {
    std::env::var("HELM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm".to_string())
}

#[tokio::test]
async fn deregister_removes_agent_and_orphan_host() {
    let db = Db::connect(&test_url()).await.expect("connect");
    db.migrate().await.expect("migrate");

    let registry = ConnectionRegistry::new();
    let service = AgentLifecycleService::new(db.clone(), registry.clone());

    // 注册一个 agent，使用唯一 hostname，避免污染其他测试数据。
    let agent_id = format!("itest-lifecycle-{}", uuid::Uuid::new_v4());
    let hostname = format!("itest-host-{}", uuid::Uuid::new_v4());
    AgentRepo::new(db.clone())
        .register(
            &agent_id,
            "0.0.0",
            &hostname,
            "linux",
            "x86_64",
            "linux-x86_64",
        )
        .await
        .expect("register");
    assert!(
        HostRepo::new(db.clone())
            .get_by_hostname(&hostname)
            .await
            .expect("get host")
            .is_some()
    );

    service.deregister(&agent_id).await.expect("deregister");

    // agent 已永久删除
    assert!(
        AgentRepo::new(db.clone())
            .get(&agent_id)
            .await
            .expect("get agent")
            .is_none()
    );
    // 孤儿 host 软删除
    assert!(
        HostRepo::new(db.clone())
            .get_by_hostname(&hostname)
            .await
            .expect("get host")
            .is_none()
    );
}
