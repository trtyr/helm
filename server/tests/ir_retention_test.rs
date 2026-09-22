//! IR 取证表保留策略集成测试（T009）：快照按「每主机条数」封顶、页面缓存按 TTL 清理。
//!
//! 两个动作都是全库范围（快照清理按 agent 分组、缓存清理按 cutoff）→ 本文件内串行，
//! 避免一条测试的 delete 把另一条刚拨旧的行吃掉（同 retention_test 的 guard 模式）。

mod common;

use helm_server::application::ir_retention::{IrRetentionReport, run_once};
use helm_server::store::Db;
use helm_server::store::ir_repo;
use std::sync::LazyLock;
use uuid::Uuid;

static IR_GUARD: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

/// 造一条快照；`age_minutes` 越大越旧（用于控制 `ORDER BY created_at DESC` 的名次）。
async fn insert_snapshot(db: &Db, agent_id: &str, label: &str, age_minutes: i32) -> Uuid {
    let findings = serde_json::json!([{ "kind": label }]);
    let id = ir_repo::insert_snapshot(db.pool(), agent_id, label, &findings, 1)
        .await
        .expect("insert snapshot");
    sqlx::query(
        "UPDATE ir_snapshots SET created_at = now() - make_interval(mins => $2) WHERE id = $1",
    )
    .bind(id)
    .bind(age_minutes)
    .execute(db.pool())
    .await
    .expect("backdate snapshot");
    id
}

async fn insert_cache(db: &Db, agent_id: &str, kind: &str, age_days: i32) {
    ir_repo::upsert_page_cache(
        db.pool(),
        agent_id,
        kind,
        &serde_json::json!([{ "kind": kind }]),
        1,
    )
    .await
    .expect("upsert page cache");
    if age_days > 0 {
        sqlx::query(
            "UPDATE ir_page_cache SET created_at = now() - make_interval(days => $3)
              WHERE agent_id = $1 AND kind = $2",
        )
        .bind(agent_id)
        .bind(kind)
        .bind(age_days)
        .execute(db.pool())
        .await
        .expect("backdate page cache");
    }
}

#[tokio::test]
async fn snapshot_retention_keeps_newest_n_per_agent() {
    let _g = IR_GUARD.lock().await;
    let db = common::connect().await;
    let agent = format!("itest-ir-{}", Uuid::new_v4());

    let newest = insert_snapshot(&db, &agent, "newest", 0).await;
    let second = insert_snapshot(&db, &agent, "second", 10).await;
    let doomed = [
        insert_snapshot(&db, &agent, "old3", 20).await,
        insert_snapshot(&db, &agent, "old4", 30).await,
        insert_snapshot(&db, &agent, "old5", 40).await,
    ];

    let deleted = ir_repo::delete_excess_snapshots(db.pool(), 2)
        .await
        .expect("delete excess snapshots");
    assert!(deleted >= 3, "本测试的 3 条旧快照应被删，实际 {deleted}");

    let left = ir_repo::list_snapshots(db.pool(), &agent)
        .await
        .expect("list snapshots");
    let ids: Vec<Uuid> = left.iter().map(|s| s.id).collect();
    assert_eq!(ids.len(), 2, "每主机只保留最近 2 条：{ids:?}");
    assert!(
        ids.contains(&newest) && ids.contains(&second),
        "保留的必须是最新两条"
    );
    for id in doomed {
        assert!(!ids.contains(&id), "旧快照必须被删：{id}");
    }
}

#[tokio::test]
async fn page_cache_retention_deletes_stale_and_keeps_fresh() {
    let _g = IR_GUARD.lock().await;
    let db = common::connect().await;
    let agent = format!("itest-ir-cache-{}", Uuid::new_v4());

    insert_cache(&db, &agent, "autostart", 90).await;
    insert_cache(&db, &agent, "syslog", 0).await;

    let cutoff = chrono::Utc::now() - chrono::Duration::days(30);
    let deleted = ir_repo::delete_stale_page_cache(db.pool(), cutoff)
        .await
        .expect("delete stale page cache");
    assert!(deleted >= 1, "超期缓存应被删：{deleted}");

    assert!(
        ir_repo::get_page_cache(db.pool(), &agent, "autostart")
            .await
            .expect("get stale")
            .is_none(),
        "超期页面缓存必须被清理"
    );
    assert!(
        ir_repo::get_page_cache(db.pool(), &agent, "syslog")
            .await
            .expect("get fresh")
            .is_some(),
        "未超期页面缓存必须保留"
    );
}

#[tokio::test]
async fn run_once_reports_both_dimensions_and_honours_zero_switch() {
    let _g = IR_GUARD.lock().await;
    let db = common::connect().await;
    let agent = format!("itest-ir-run-{}", Uuid::new_v4());

    for i in 0..3 {
        insert_snapshot(&db, &agent, &format!("s{i}"), i * 10).await;
    }
    insert_cache(&db, &agent, "autostart", 90).await;

    let report = run_once(&db, 1, 30).await;
    assert!(report.snapshots >= 2, "超额快照应被清理：{report:?}");
    assert!(report.page_cache >= 1, "超期缓存应被清理：{report:?}");
    assert_eq!(report.total(), report.snapshots + report.page_cache);

    let kept = ir_repo::list_snapshots(db.pool(), &agent)
        .await
        .expect("list after cleanup");
    assert_eq!(kept.len(), 1, "keep=1 → 只剩最新 1 条");

    // 0 = 不清理：两条配置各自跳过，且不得删任何东西
    let zero = run_once(&db, 0, 0).await;
    assert_eq!(
        zero,
        IrRetentionReport::default(),
        "0/0 必须是零动作（不清理开关）"
    );
    let after = ir_repo::list_snapshots(db.pool(), &agent)
        .await
        .expect("list after zero switch");
    assert_eq!(after.len(), kept.len(), "0 开关下快照数不变");
}
