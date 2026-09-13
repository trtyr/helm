//! 通知中心集成测试：连专用临时库（helm_itest，见 tests/common） 验证 notification_repo 读写、
//! 冷却窗口合并（notify 同类型去重 / 不同类型独立）与已读未读。
//! 需 Postgres（docker compose 5433），测试库自动重建。

mod common;

use helm_proto::pb::{AgentMessage, MetricPoint, MetricReport, agent_message};
use helm_server::application::notification_service::{
    KIND_ALERT, KIND_OFFLINE, KIND_ONLINE, NotificationService,
};
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::grpc::file_list_registry::FileListRegistry;
use helm_server::grpc::inbound::InboundCtx;
use helm_server::grpc::query_registry::QueryRegistry;
use helm_server::grpc::session_registry::SessionRegistry;
use helm_server::grpc::stream_registry::StreamRegistry;
use helm_server::grpc::transfer_registry::TransferRegistry;
use helm_server::store::Db;
use helm_server::store::host_repo::{HostRepo, NewHost};
use helm_server::store::notification_repo::NotificationRepo;
use std::sync::LazyLock;
use tokio::sync::Mutex;
use uuid::Uuid;

/// mark_all_read 是全局口径，两个对未读计数敏感的测试用互斥串行，
/// 避免并行线程互相清零对方刚制造的未读行。
static READ_STATE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

async fn setup_host(db: &Db, hostname: &str) -> Uuid {
    let host = NewHost {
        hostname: hostname.into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        platform: "linux-x86_64".into(),
        tags: vec![],
        conn_mode: "reverse".into(),
        addr: String::new(),
    };
    HostRepo::new(db.clone())
        .insert(&host)
        .await
        .expect("insert host")
        .id
}

async fn cleanup(db: &Db, hostname: &str) {
    // 硬删测试数据（notifications 对 host ON DELETE CASCADE）
    sqlx::query("DELETE FROM hosts WHERE hostname = $1")
        .bind(hostname)
        .execute(db.pool())
        .await
        .expect("cleanup host");
}

/// 按本测试 host 过滤计数（count/list 是全局口径，避免并行测试与历史残留污染断言）。
async fn count_for(repo: &NotificationRepo, host_id: Uuid, unread_only: bool) -> usize {
    repo.list_paged(1000, 0, unread_only)
        .await
        .unwrap()
        .iter()
        .filter(|r| r.host_id == host_id)
        .count()
}

#[tokio::test]
async fn inbound_metric_over_threshold_triggers_alert_notification() {
    let db = common::connect().await;
    let hostname = format!("itest-notif-alert-{}", std::process::id());
    let host_id = setup_host(&db, &hostname).await;
    let repo = NotificationRepo::new(db.clone());

    // 构造 InboundCtx（reverse/forward 共用的入站处理），喂超阈值指标
    let mut ctx = InboundCtx::new(
        format!("agent-{}", std::process::id()),
        Some(host_id),
        hostname.clone(),
        ConnectionRegistry::new(),
        tokio::sync::watch::channel(false).0,
        TransferRegistry::new(),
        SessionRegistry::new(),
        FileListRegistry::new(),
        QueryRegistry::new(),
        StreamRegistry::new(),
        db.clone(),
    );
    ctx.handle(AgentMessage {
        kind: Some(agent_message::Kind::MetricReport(MetricReport {
            metrics: vec![MetricPoint {
                name: "cpu.usage".into(),
                value: 95.5,
                labels: Default::default(),
                timestamp_unix_ms: 1800000000000,
            }],
        })),
    })
    .await;

    // 预警联动：alert 类型通知产生（含指标名与实测值）
    let note = repo.latest_of_type(host_id, KIND_ALERT).await.unwrap();
    let note = note.expect("alert notification should exist");
    assert!(note.message.contains("cpu.usage"), "msg: {}", note.message);
    assert!(note.message.contains("95.5"), "msg: {}", note.message);

    // 阈值内指标不触发任何通知
    ctx.handle(AgentMessage {
        kind: Some(agent_message::Kind::MetricReport(MetricReport {
            metrics: vec![MetricPoint {
                name: "mem.percent".into(),
                value: 42.0,
                labels: Default::default(),
                timestamp_unix_ms: 1800000000001,
            }],
        })),
    })
    .await;
    assert!(
        repo.latest_of_type(host_id, KIND_OFFLINE)
            .await
            .unwrap()
            .is_none(),
        "in-range metric must not notify"
    );

    cleanup(&db, &hostname).await;
}

#[tokio::test]
async fn repo_latest_of_type_is_type_independent() {
    let db = common::connect().await;
    let hostname = format!("itest-notif-{}", std::process::id());
    let host_id = setup_host(&db, &hostname).await;
    let repo = NotificationRepo::new(db.clone());

    repo.insert(host_id, KIND_ONLINE, "up")
        .await
        .expect("insert online");
    repo.insert(host_id, KIND_OFFLINE, "down")
        .await
        .expect("insert offline");
    repo.insert(host_id, KIND_ALERT, "cpu high")
        .await
        .expect("insert alert");

    // 不同类型各自独立：latest_of_type 按类型返回各自最近一条
    let online = repo
        .latest_of_type(host_id, KIND_ONLINE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(online.kind, KIND_ONLINE);
    assert_eq!(online.message, "up");
    let offline = repo
        .latest_of_type(host_id, KIND_OFFLINE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(offline.kind, KIND_OFFLINE);
    assert_eq!(offline.message, "down");

    cleanup(&db, &hostname).await;
}

#[tokio::test]
async fn notify_coalesces_same_kind_within_window() {
    let _serial = READ_STATE_LOCK.lock().await;
    let db = common::connect().await;
    let hostname = format!("itest-notif-cool-{}", std::process::id());
    let host_id = setup_host(&db, &hostname).await;
    let repo = NotificationRepo::new(db.clone());
    let svc = NotificationService::new(db.clone(), StreamRegistry::new());

    // 同类型两次（必然落在 5 分钟窗口内）→ 合并为一条，消息刷新
    svc.notify(host_id, KIND_ONLINE, "host up (1st)")
        .await
        .expect("notify 1");
    svc.notify(host_id, KIND_ONLINE, "host up (2nd)")
        .await
        .expect("notify 2");
    assert_eq!(count_for(&repo, host_id, false).await, 1);
    let row = repo
        .latest_of_type(host_id, KIND_ONLINE)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.message, "host up (2nd)");

    // 已读后窗口内再合并 → 重置为未读（新事件发生）
    repo.mark_read(row.id).await.expect("mark read");
    svc.notify(host_id, KIND_ONLINE, "host up (3rd)")
        .await
        .expect("notify 3");
    let row = repo
        .latest_of_type(host_id, KIND_ONLINE)
        .await
        .unwrap()
        .unwrap();
    assert!(!row.read, "cooldown refresh should reset read state");
    assert_eq!(count_for(&repo, host_id, true).await, 1);

    // 不同类型不受冷却窗口影响 → 独立成行
    svc.notify(host_id, KIND_OFFLINE, "host down")
        .await
        .expect("notify offline");
    assert_eq!(count_for(&repo, host_id, false).await, 2);

    cleanup(&db, &hostname).await;
}

#[tokio::test]
async fn list_unread_filter_and_mark_all_read() {
    let _serial = READ_STATE_LOCK.lock().await;
    let db = common::connect().await;
    let hostname = format!("itest-notif-read-{}", std::process::id());
    let host_id = setup_host(&db, &hostname).await;
    let repo = NotificationRepo::new(db.clone());
    let svc = NotificationService::new(db.clone(), StreamRegistry::new());

    svc.notify(host_id, KIND_ONLINE, "up")
        .await
        .expect("notify");
    svc.notify(host_id, KIND_OFFLINE, "down")
        .await
        .expect("notify");
    svc.notify(host_id, KIND_ALERT, "cpu 95%")
        .await
        .expect("notify");

    // unread 过滤 + 分页（按本 host 过滤断言）
    assert_eq!(count_for(&repo, host_id, true).await, 3);
    let unread = repo
        .list_paged(1000, 0, true)
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.host_id == host_id)
        .collect::<Vec<_>>();
    assert_eq!(unread.len(), 3);
    assert!(unread.iter().all(|r| !r.read));

    // 单条已读 → 未读数 2；全部已读 → 本 host 未读 0
    repo.mark_read(unread[0].id).await.expect("mark one");
    assert_eq!(count_for(&repo, host_id, true).await, 2);
    svc.mark_all_read().await.expect("mark all");
    assert_eq!(count_for(&repo, host_id, true).await, 0);

    cleanup(&db, &hostname).await;
}
