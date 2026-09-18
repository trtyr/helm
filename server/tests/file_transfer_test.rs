//! 文件传输集成测试：校验失败 / agent 失败时 file_transfers.status 忠实落 'failed'（EN-65）。
//!
//! 不需要真实 agent 连接：ConnectionRegistry 注册内存假通道承接下发消息，
//! TransferRegistry 由测试持有（FileService 构造注入），直接模拟 agent 侧回报
//! FileStatus / download chunk——与 grpc/inbound.rs 的真实路径同构。

mod common;

use helm_proto::pb::{FileStatus, ServerMessage, file_status};
use helm_server::application::file_service::{FileService, checksum, resolve_status};
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::grpc::file_list_registry::FileListRegistry;
use helm_server::grpc::transfer_registry::TransferRegistry;
use helm_server::store::agent_repo::{AgentRepo, HostOsDetails};
use tokio::sync::mpsc;

/// 注册 agent（DB 行 + 假连接），返回 (agent_id, host_id, 下行消息接收端)。
async fn seed_agent(
    db: &helm_server::store::Db,
    registry: &ConnectionRegistry,
) -> (String, uuid::Uuid, mpsc::Receiver<ServerMessage>) {
    let agent_id = format!("itest-file-{}", uuid::Uuid::new_v4());
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
            &[],
            false,
            HostOsDetails::default(),
        )
        .await
        .expect("register agent");
    let host_id = AgentRepo::new(db.clone())
        .get_host_id(&agent_id)
        .await
        .expect("get_host_id")
        .expect("host registered");
    let (msg_tx, msg_rx) = mpsc::channel::<ServerMessage>(64);
    registry.register(&agent_id, msg_tx).await.unwrap();
    (agent_id, host_id, msg_rx)
}

/// 等到 FileRequest 并返回其 transfer_id（跳过其他消息形态以防脆弱）。
async fn recv_transfer_id(rx: &mut mpsc::Receiver<ServerMessage>) -> String {
    loop {
        let msg = tokio::time::timeout(std::time::Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out waiting for FileRequest")
            .expect("agent channel closed");
        if let Some(helm_proto::pb::server_message::Kind::FileRequest(req)) = msg.kind {
            return req.transfer_id;
        }
    }
}

fn file_status(transfer_id: &str, state: file_status::State, checksum: &str) -> FileStatus {
    FileStatus {
        transfer_id: transfer_id.to_string(),
        state: state as i32,
        bytes_transferred: 0,
        error: String::new(),
        checksum: checksum.to_string(),
    }
}

/// 按 host_id 取最近一条传输行（每个测试独占新 host，恰为一行）。
/// 注：不能按 upload() 返回的 transfer_id 查——那是协议传输 ID，与 file_transfers.id 无关联（既有语义）。
async fn find_by_host(
    db: &helm_server::store::Db,
    host_id: uuid::Uuid,
) -> helm_server::store::file_transfer_repo::FileTransferRow {
    sqlx::query_as::<_, helm_server::store::file_transfer_repo::FileTransferRow>(
        "SELECT id, host_id, direction, path, size, bytes_transferred, checksum, status
         FROM file_transfers WHERE host_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(host_id)
    .fetch_one(db.pool())
    .await
    .expect("transfer row exists")
}

#[tokio::test]
async fn upload_checksum_mismatch_persists_failed() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let transfers = TransferRegistry::new();
    let service = FileService::new(
        db.clone(),
        registry.clone(),
        transfers.clone(),
        FileListRegistry::new(),
    );
    let (agent_id, host_id, mut msg_rx) = seed_agent(&db, &registry).await;

    // 本地源文件
    let src = std::env::temp_dir().join(format!("helm-itest-src-{}.bin", uuid::Uuid::new_v4()));
    let payload = b"hello transfer";
    tokio::fs::write(&src, payload).await.expect("write source");

    let expected = checksum(payload);
    let handle = tokio::spawn({
        let service = service.clone();
        let src = src.clone();
        async move {
            service
                .upload(&agent_id, &src.to_string_lossy(), "/tmp/itest.bin")
                .await
        }
    });

    let transfer_id = recv_transfer_id(&mut msg_rx).await;
    // 模拟 agent 报 Done 但 checksum 与本地期望不一致（传输损坏）
    transfers
        .complete(
            &transfer_id,
            file_status(&transfer_id, file_status::State::Done, "deadbeef"),
        )
        .await;

    let (_id, ok) = handle
        .await
        .expect("join upload")
        .expect("upload ok result");
    assert!(!ok, "checksum mismatch must report ok=false");
    let _ = tokio::fs::remove_file(&src).await;

    let row = find_by_host(&db, host_id).await;
    assert_eq!(
        row.status, "failed",
        "EN-65: mismatch must persist 'failed', got {:?}",
        row.status
    );
    assert_eq!(row.checksum.as_deref(), Some("deadbeef"));
    assert_eq!(row.size, payload.len() as i64);
    // 行内 sanity：期望值本身校验逻辑无误
    assert_ne!(expected, "deadbeef");
}

#[tokio::test]
async fn upload_agent_failure_persists_failed() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let transfers = TransferRegistry::new();
    let service = FileService::new(
        db.clone(),
        registry.clone(),
        transfers.clone(),
        FileListRegistry::new(),
    );
    let (agent_id, host_id, mut msg_rx) = seed_agent(&db, &registry).await;

    let src = std::env::temp_dir().join(format!("helm-itest-src-{}.bin", uuid::Uuid::new_v4()));
    tokio::fs::write(&src, b"payload")
        .await
        .expect("write source");

    let handle = tokio::spawn({
        let service = service.clone();
        let src = src.clone();
        async move {
            service
                .upload(&agent_id, &src.to_string_lossy(), "/tmp/itest.bin")
                .await
        }
    });

    let transfer_id = recv_transfer_id(&mut msg_rx).await;
    // 模拟 agent 侧读/写盘失败：Failed 态 + 空 checksum（agent/src/file.rs 失败路径）
    transfers
        .complete(
            &transfer_id,
            file_status(&transfer_id, file_status::State::Failed, ""),
        )
        .await;

    let (_id, ok) = handle
        .await
        .expect("join upload")
        .expect("upload ok result");
    assert!(!ok, "agent failure must report ok=false");
    let _ = tokio::fs::remove_file(&src).await;

    let row = find_by_host(&db, host_id).await;
    assert_eq!(
        row.status, "failed",
        "EN-65: agent failure must persist 'failed'"
    );
}

#[tokio::test]
async fn upload_success_still_persists_done() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let transfers = TransferRegistry::new();
    let service = FileService::new(
        db.clone(),
        registry.clone(),
        transfers.clone(),
        FileListRegistry::new(),
    );
    let (agent_id, host_id, mut msg_rx) = seed_agent(&db, &registry).await;

    let src = std::env::temp_dir().join(format!("helm-itest-src-{}.bin", uuid::Uuid::new_v4()));
    let payload = b"clean payload";
    tokio::fs::write(&src, payload).await.expect("write source");

    let handle = tokio::spawn({
        let service = service.clone();
        let src = src.clone();
        async move {
            service
                .upload(&agent_id, &src.to_string_lossy(), "/tmp/itest.bin")
                .await
        }
    });

    let transfer_id = recv_transfer_id(&mut msg_rx).await;
    // 模拟 agent 报 Done 且 checksum 一致（成功路径）
    transfers
        .complete(
            &transfer_id,
            file_status(&transfer_id, file_status::State::Done, &checksum(payload)),
        )
        .await;

    let (_id, ok) = handle
        .await
        .expect("join upload")
        .expect("upload ok result");
    assert!(ok, "matching checksum must report ok=true");
    let _ = tokio::fs::remove_file(&src).await;

    let row = find_by_host(&db, host_id).await;
    assert_eq!(row.status, "done", "success path must keep writing 'done'");
}

#[tokio::test]
async fn download_checksum_mismatch_persists_failed_and_skips_local_write() {
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let transfers = TransferRegistry::new();
    let service = FileService::new(
        db.clone(),
        registry.clone(),
        transfers.clone(),
        FileListRegistry::new(),
    );
    let (agent_id, host_id, mut msg_rx) = seed_agent(&db, &registry).await;

    let dst = std::env::temp_dir().join(format!("helm-itest-dst-{}.bin", uuid::Uuid::new_v4()));

    let handle = tokio::spawn({
        let service = service.clone();
        let dst = dst.clone();
        async move {
            service
                .download(&agent_id, "/remote/data.bin", &dst.to_string_lossy())
                .await
        }
    });

    let transfer_id = recv_transfer_id(&mut msg_rx).await;
    // 模拟 agent 上行：残缺数据 + 自称 Done 的不一致 checksum
    transfers
        .accumulate_chunk(&transfer_id, b"corrupted data")
        .await;
    transfers
        .complete(
            &transfer_id,
            file_status(&transfer_id, file_status::State::Done, "agentsum"),
        )
        .await;

    let (_id, ok) = handle
        .await
        .expect("join download")
        .expect("download ok result");
    assert!(!ok, "download mismatch must report ok=false");
    assert!(
        !dst.exists(),
        "EN-65: corrupted download must not be written to local disk"
    );

    let row = find_by_host(&db, host_id).await;
    assert_eq!(
        row.status, "failed",
        "EN-65: download mismatch must persist 'failed'"
    );
}

#[tokio::test]
async fn transfer_timeout_persists_failed() {
    // B3③：agent「流未断但不回 FileStatus」——oneshot 等待超时，HTTP 快速失败且库落 failed。
    let db = common::connect().await;
    let registry = ConnectionRegistry::new();
    let transfers = TransferRegistry::new();
    let service = FileService::new(
        db.clone(),
        registry.clone(),
        transfers.clone(),
        FileListRegistry::new(),
    )
    .with_transfer_timeout(std::time::Duration::from_millis(100));
    let (agent_id, host_id, _msg_rx) = seed_agent(&db, &registry).await;

    let src = std::env::temp_dir().join(format!("helm-itest-src-{}.bin", uuid::Uuid::new_v4()));
    tokio::fs::write(&src, b"payload")
        .await
        .expect("write source");

    let started = std::time::Instant::now();
    let result = service
        .upload(&agent_id, &src.to_string_lossy(), "/tmp/itest.bin")
        .await;
    assert!(result.is_err(), "silent agent must time out");
    assert!(
        started.elapsed() < std::time::Duration::from_secs(5),
        "timeout must be bounded, took {:?}",
        started.elapsed()
    );
    let _ = tokio::fs::remove_file(&src).await;

    let row = find_by_host(&db, host_id).await;
    assert_eq!(
        row.status, "failed",
        "timed-out transfer must persist 'failed'"
    );
}

#[tokio::test]
async fn upload_send_failure_persists_failed() {
    // agent 有 DB 行但无活跃连接：registry.send 失败 → Err(NotConnected)，且库行落 failed
    //（不再遗留 pending 孤行，风险债 B3-①）。
    let db = common::connect().await;
    let registry = ConnectionRegistry::new(); // 空注册表：不注册任何连接
    let transfers = TransferRegistry::new();
    let service = FileService::new(
        db.clone(),
        registry.clone(),
        transfers.clone(),
        FileListRegistry::new(),
    );

    let agent_id = format!("itest-file-offline-{}", uuid::Uuid::new_v4());
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
            &[],
            false,
            HostOsDetails::default(),
        )
        .await
        .expect("register agent");
    let host_id = AgentRepo::new(db.clone())
        .get_host_id(&agent_id)
        .await
        .expect("get_host_id")
        .expect("host registered");

    let src = std::env::temp_dir().join(format!("helm-itest-src-{}.bin", uuid::Uuid::new_v4()));
    tokio::fs::write(&src, b"payload")
        .await
        .expect("write source");

    let result = service
        .upload(&agent_id, &src.to_string_lossy(), "/tmp/itest.bin")
        .await;
    assert!(
        result.is_err(),
        "offline agent must yield NotConnected error"
    );
    let _ = tokio::fs::remove_file(&src).await;

    let row = find_by_host(&db, host_id).await;
    assert_eq!(
        row.status, "failed",
        "EN-65/B3-①: send failure must persist 'failed', got {:?}",
        row.status
    );
}

#[tokio::test]
async fn resolve_status_covers_three_outcomes() {
    // 纯函数三分支兜底（不依赖 DB）
    let expected = "abc";
    let (done, ok) = resolve_status(file_status::State::Done as i32, expected, expected);
    assert_eq!(done, "done");
    assert!(ok);
    let (mismatch, ok) = resolve_status(file_status::State::Done as i32, "zzz", expected);
    assert_eq!(mismatch, "failed");
    assert!(!ok);
    let (failed, ok) = resolve_status(file_status::State::Failed as i32, "", expected);
    assert_eq!(failed, "failed");
    assert!(!ok);
}
