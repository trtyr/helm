//! Agent 生命周期集成测试：注销删除 agent + 孤儿 host。

mod common;

use helm_server::application::agent_lifecycle_service::AgentLifecycleService;
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::store::agent_repo::AgentRepo;
use helm_server::store::host_repo::HostRepo;


#[tokio::test]
async fn deregister_removes_agent_and_orphan_host() {
    let db = common::connect().await;

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
            "203.0.113.7",
            &["192.168.1.10".to_string()],
            false,
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
