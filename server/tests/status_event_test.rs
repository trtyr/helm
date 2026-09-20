//! 状态事件集成测试（P003 T1）：insert / list_paged / count_filtered 的
//! filter 镜像一致性（count 与 list 同过滤范围必须相等——审计不变式）。

mod common;

use helm_server::application::notification_service::{
    NotificationService, handle_stale_row, record_online_and_notify,
};
use helm_server::grpc::connection_registry::ConnectionRegistry;
use helm_server::grpc::stream_registry::StreamRegistry;
use helm_server::store::agent_repo::StaleAgentRow;
use helm_server::store::status_event_repo::StatusEventRepo;
use std::sync::LazyLock;

/// status_events 全测试共享同一张表，count/list 镜像断言要求表状态稳定——
/// 三个测试经此锁串行（同 notification_test::READ_STATE_LOCK 模式）。
static SE_GUARD: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

#[tokio::test]
async fn count_and_list_mirror_filters() {
    let _g = SE_GUARD.lock().await;
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db);

    // 唯一前缀隔离：并行测试库共享，不与其他 fixture 碰撞
    repo.insert("se-alpha", "online", "se_registered", "")
        .await
        .unwrap();
    repo.insert(
        "se-alpha",
        "offline",
        "se_mirror_transport_error",
        "h2 connection reset",
    )
    .await
    .unwrap();
    repo.insert("se-beta", "online", "se_registered", "")
        .await
        .unwrap();

    // 全量可见（不假设表为空——并行库共享）
    let all = repo
        .list_paged(None, None, None, None, None, None, 100, 0)
        .await
        .unwrap();
    let total = repo
        .count_filtered(None, None, None, None, None)
        .await
        .unwrap();
    assert!(all.len() >= 3, "至少 3 条 fixture: {}", all.len());
    assert_eq!(all.len() as i64, total, "list 长度必须等于 count（全量）");

    // host 过滤：count 与 list 长度镜像
    let list_a = repo
        .list_paged(None, Some("se-alpha"), None, None, None, None, 100, 0)
        .await
        .unwrap();
    let count_a = repo
        .count_filtered(None, Some("se-alpha"), None, None, None)
        .await
        .unwrap();
    assert_eq!(list_a.len() as i64, count_a, "host 过滤 count/list 镜像");
    assert_eq!(count_a, 2);

    // q 过滤：命中 reason（count/list 镜像 + 内容断言；reason 值本测试独有，防跨测试残留）
    let list_q = repo
        .list_paged(Some("se_mirror"), None, None, None, None, None, 100, 0)
        .await
        .unwrap();
    let count_q = repo
        .count_filtered(Some("se_mirror"), None, None, None, None)
        .await
        .unwrap();
    assert_eq!(list_q.len() as i64, count_q, "q 过滤 count/list 镜像");
    assert_eq!(count_q, 1);
    assert_eq!(list_q[0].reason, "se_mirror_transport_error");
    assert_eq!(list_q[0].detail, "h2 connection reset");

    // se-beta 行存在（全表共享，不假设 all[0] 是谁——其他测试随时插入新行）
    assert!(
        all.iter()
            .any(|r| r.host_id == "se-beta" && r.event == "online")
    );
    // 默认排序 created_at DESC：se-alpha 的后插 offline 在前（过滤范围内验证排序稳定）
    assert_eq!(
        list_a[0].event, "offline",
        "created_at DESC：后插的 offline 在前"
    );
    assert_eq!(list_a[1].event, "online");
}

#[tokio::test]
async fn offline_alert_sweeper_escalates_expired_host() {
    let _g = SE_GUARD.lock().await;
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db.clone());

    // 真 host（alerts.host_id FK 需要）
    let host = helm_server::store::host_repo::NewHost {
        hostname: "se-alert-host".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        platform: "linux-x86_64".into(),
        tags: vec![],
        conn_mode: "reverse".into(),
        addr: String::new(),
    };
    let host_id = helm_server::store::host_repo::HostRepo::new(db.clone())
        .insert(&host)
        .await
        .expect("insert host")
        .id;
    let hid = host_id.to_string();

    repo.insert(&hid, "online", "se_registered", "")
        .await
        .unwrap();
    repo.insert(&hid, "offline", "se_transport_error", "")
        .await
        .unwrap();
    // online 回填 45 分钟前，offline 回填 40 分钟前（超 30 分钟阈值且 offline 是最新事件）
    sqlx::query("UPDATE status_events SET created_at = now() - interval '45 minutes' WHERE host_id = $1 AND event = 'online'")
        .bind(&hid)
        .execute(db.pool())
        .await
        .unwrap();
    sqlx::query("UPDATE status_events SET created_at = now() - interval '40 minutes' WHERE host_id = $1 AND event = 'offline'")
        .bind(&hid)
        .execute(db.pool())
        .await
        .unwrap();

    let n = helm_server::application::offline_alert_sweeper::sweep_once(&db, 30)
        .await
        .unwrap();
    assert!(n >= 1, "超阈值离线应触发升级（全局计数 >= 1）");

    // escalated 已标记
    let latest = repo.latest_per_host().await.unwrap();
    let row = latest.iter().find(|r| r.host_id == hid).unwrap();
    assert_eq!(row.event, "offline");
    assert!(row.escalated, "升级后事件应标记 escalated");

    // alerts 真实写入
    let (alerts,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM alerts WHERE metric_name = 'host_offline' AND host_id = $1",
    )
    .bind(host_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(alerts, 1);

    // 幂等：再扫一轮不重复触发（该 host 已标记）
    let _ = helm_server::application::offline_alert_sweeper::sweep_once(&db, 30)
        .await
        .unwrap();
    let (after,): (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM alerts WHERE metric_name = 'host_offline' AND host_id = $1",
    )
    .bind(host_id)
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(alerts, after, "已升级 host 不应重复触发");
}

#[tokio::test]
async fn offline_alert_sweeper_respects_threshold_and_disable() {
    let _g = SE_GUARD.lock().await;
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db.clone());

    // 未超阈值：offline 5 分钟前（< 30 分钟阈值）
    repo.insert("se-fresh", "offline", "se_transport_error", "")
        .await
        .unwrap();
    sqlx::query("UPDATE status_events SET created_at = now() - interval '5 minutes' WHERE host_id = 'se-fresh'")
        .execute(db.pool())
        .await
        .unwrap();

    let _ = helm_server::application::offline_alert_sweeper::sweep_once(&db, 30)
        .await
        .unwrap();
    let latest = repo.latest_per_host().await.unwrap();
    let fresh = latest.iter().find(|r| r.host_id == "se-fresh").unwrap();
    assert!(!fresh.escalated, "阈值内不升级");

    // 禁用：mins=0 直接返回 0 且不升级任何行
    let n0 = helm_server::application::offline_alert_sweeper::sweep_once(&db, 0)
        .await
        .unwrap();
    assert_eq!(n0, 0, "禁用分支应返回 0");
    let latest = repo.latest_per_host().await.unwrap();
    assert!(
        latest
            .iter()
            .all(|r| !r.escalated || r.host_id != "se-fresh"),
        "禁用模式下 se-fresh 不应被标记"
    );
}

/// 审计缺陷修复回归（2026-09-20 首次审计拒绝点①）：上线落库 + 通知的唯一实现。
/// 此前 forward 模式只发通知、不落状态事件——本测试锁住共享函数的两个副作用，
/// 使 reverse/forward 不可能再分叉。
#[tokio::test]
async fn record_online_writes_event_and_notification() {
    let _g = SE_GUARD.lock().await;
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db.clone());

    let host = helm_server::store::host_repo::NewHost {
        hostname: "se-online-host".into(),
        os: "linux".into(),
        arch: "x86_64".into(),
        platform: "linux-x86_64".into(),
        tags: vec![],
        conn_mode: "forward".into(),
        addr: String::new(),
    };
    let host_id = helm_server::store::host_repo::HostRepo::new(db.clone())
        .insert(&host)
        .await
        .expect("insert host")
        .id;

    record_online_and_notify(&db, &StreamRegistry::new(), host_id, "se-online-host").await;

    // ① 状态事件：该 host 最新事件 = online/registered
    let latest = repo.latest_per_host().await.unwrap();
    let mine = latest
        .iter()
        .find(|r| r.host_id == host_id.to_string())
        .expect("online 事件应落库（forward 亦同）");
    assert_eq!(mine.event, "online");
    assert_eq!(mine.reason, "registered");

    // ② 通知：同 host 存在 online 通知——事件与通知同点同语义
    let (notes,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM notifications WHERE host_id = $1 AND type = 'online'")
            .bind(host_id)
            .fetch_one(db.pool())
            .await
            .unwrap();
    assert_eq!(notes, 1, "上线通知应落库且只有一条");
}

/// 审计缺陷修复回归（2026-09-20 首次审计拒绝点②）：静默掉线（心跳超时）必须落 offline 事件。
/// 此前该路径只发通知不落库，且注销早于 h2 流关闭使 on_disconnect 被 unregister_if_current 守卫
/// 挡掉——结果最新事件停在 online，offline_alert_sweeper 的 row.event != "offline" 判断永远跳过，
/// 升级告警对最该告警的场景失效。本测试直驱半开清扫的单行处置，并验证升级链真的被触发。
#[tokio::test]
async fn silent_death_persists_offline_and_escalates() {
    let _g = SE_GUARD.lock().await;
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db.clone());

    let host = helm_server::store::host_repo::NewHost {
        hostname: "se-silent-host".into(),
        os: "windows".into(),
        arch: "x86_64".into(),
        platform: "windows-x86_64".into(),
        tags: vec![],
        conn_mode: "reverse".into(),
        addr: String::new(),
    };
    let host_id = helm_server::store::host_repo::HostRepo::new(db.clone())
        .insert(&host)
        .await
        .expect("insert host")
        .id;

    // 心跳 40 秒前（timeout=30 → 落在 [30,60) 的「刚失联」窗口），注册表已无该连接
    let row = StaleAgentRow {
        id: "se-silent-agent".into(),
        host_id,
        hostname: "se-silent-host".into(),
        last_heartbeat_at: Some(chrono::Utc::now() - chrono::Duration::seconds(40)),
    };
    let svc = NotificationService::new(db.clone(), StreamRegistry::new());
    handle_stale_row(
        &db,
        &ConnectionRegistry::new(),
        &svc,
        &row,
        chrono::Utc::now(),
        30,
    )
    .await;

    // ① offline 事件落库（修复点：此前只有通知、没有事件）
    let latest = repo.latest_per_host().await.unwrap();
    let mine = latest
        .iter()
        .find(|r| r.host_id == host_id.to_string())
        .expect("静默掉线应落 offline 事件");
    assert_eq!(mine.event, "offline");
    assert_eq!(mine.reason, "heartbeat_timeout");

    // ② 回填 40 分钟前 → 升级链真的触发（证明 T2 对静默死亡场景有效）
    sqlx::query(
        "UPDATE status_events SET created_at = now() - interval '40 minutes' \
         WHERE host_id = $1 AND event = 'offline'",
    )
    .bind(host_id.to_string())
    .execute(db.pool())
    .await
    .unwrap();
    let n = helm_server::application::offline_alert_sweeper::sweep_once(&db, 30)
        .await
        .unwrap();
    assert!(n >= 1, "超阈值静默掉线应触发升级");
    let latest = repo.latest_per_host().await.unwrap();
    let mine = latest
        .iter()
        .find(|r| r.host_id == host_id.to_string())
        .unwrap();
    assert!(mine.escalated, "该 host 的 offline 事件应被标记 escalated");
}
