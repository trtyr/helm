//! 监听器集成测试：连真实 Postgres 验证 listener_repo + listener_service 启停。

use helm_server::application::listener_service::ListenerService;
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::grpc::file_list_registry::FileListRegistry;
use helm_server::grpc::listener_registry::ListenerRegistry;
use helm_server::grpc::query_registry::QueryRegistry;
use helm_server::grpc::session_registry::SessionRegistry;
use helm_server::grpc::transfer_registry::TransferRegistry;
use helm_server::store::Db;
use helm_server::store::listener_repo::ListenerRepo;

fn test_url() -> String {
    std::env::var("HELM_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm".to_string())
}

/// 找一个空闲端口，返回 `127.0.0.1:{port}`。
fn free_addr() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = l.local_addr().unwrap().port();
    drop(l);
    format!("127.0.0.1:{port}")
}

#[tokio::test]
async fn listener_repo_create_list_set_status() {
    let db = Db::connect(&test_url()).await.expect("connect");
    db.migrate().await.expect("migrate");
    let repo = ListenerRepo::new(db);

    let row = repo
        .create("itest-listener", "127.0.0.1:15999", "grpc", "")
        .await
        .expect("create");
    assert_eq!(row.name, "itest-listener");
    assert_eq!(row.status, "stopped");

    let rows = repo.list().await.expect("list");
    assert!(rows.iter().any(|r| r.id == row.id));

    repo.set_status(row.id, "running")
        .await
        .expect("set_status");
    let got = repo.get(row.id).await.expect("get").expect("row");
    assert_eq!(got.status, "running");

    // 清理
    let _ = repo.delete(row.id).await;
}

#[tokio::test]
async fn listener_service_create_start_stop() {
    let db = Db::connect(&test_url()).await.expect("connect");
    db.migrate().await.expect("migrate");

    let registry = ConnectionRegistry::new();
    let transfers = TransferRegistry::new();
    let listeners = ListenerRegistry::new();
    let service = ListenerService::new(
        db.clone(),
        listeners.clone(),
        registry.clone(),
        transfers.clone(),
        SessionRegistry::new(),
        FileListRegistry::new(),
        QueryRegistry::new(),
        "fallback-token".into(),
    );

    let addr = free_addr();
    let created = service
        .create("itest-svc", &addr, "grpc", "")
        .await
        .expect("create");
    assert!(!created.running);

    service.start(created.id).await.expect("start");
    let views = service.list().await.expect("list");
    let v = views.iter().find(|v| v.id == created.id).expect("found");
    assert!(v.running);
    assert_eq!(v.status, "running");

    service.stop(created.id).await.expect("stop");
    let views = service.list().await.expect("list");
    let v = views.iter().find(|v| v.id == created.id).expect("found");
    assert!(!v.running);
    assert_eq!(v.status, "stopped");

    // 清理
    let _ = ListenerRepo::new(db).delete(created.id).await;
}
