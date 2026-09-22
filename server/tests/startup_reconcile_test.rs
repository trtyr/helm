//! 重启对账集成测试（T007）：上一进程遗留的在飞状态必须被对齐为 `failed`。
//!
//! 夹具造两类「遗留」行（`running`/`queued` job、`pending`/`transferring` 传输）与
//! 若干「已终态」行作对照——对账只能动前者。同进程共享 `helm_itest`，故 fixture 带
//! 唯一前缀，断言只针对本测试造的行（不做整库全量断言，避免并行互相干扰）。

mod common;

use helm_server::application::startup_reconcile::{ORPHAN_REASON, reconcile_orphans};
use helm_server::store::Db;
use std::sync::LazyLock;
use uuid::Uuid;

/// 对账是全库范围的动作（`WHERE status IN ('running','queued')`），本文件两个 DB 测试
/// 共享同一张表 → 必须串行，否则先跑的那个会把后者的 fixture 一并扫掉
/// （同 status_event_test::SE_GUARD 模式）。
static RECONCILE_GUARD: LazyLock<tokio::sync::Mutex<()>> =
    LazyLock::new(|| tokio::sync::Mutex::new(()));

async fn make_host(db: &Db, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO hosts (hostname, os, arch, platform)
         VALUES ($1, 'linux', 'x86_64', 'linux') RETURNING id",
    )
    .bind(name)
    .fetch_one(db.pool())
    .await
    .expect("insert host")
}

/// 造 job：`running` 带上 10 分钟前的 `started_at`——正是会被 sweeper 误判成
/// `timed_out` 的那种形态（假超时）。
async fn make_job(db: &Db, host_id: Uuid, status: &str, output: Option<&str>) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO jobs (host_id, status, command, output, started_at)
         VALUES ($1, $2, 'echo reconciled', $3,
                 CASE WHEN $2 = 'running' THEN now() - interval '10 minutes' END)
         RETURNING id",
    )
    .bind(host_id)
    .bind(status)
    .bind(output)
    .fetch_one(db.pool())
    .await
    .expect("insert job")
}

async fn make_transfer(db: &Db, host_id: Uuid, status: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO file_transfers (host_id, direction, path, size, status)
         VALUES ($1, 'upload', '/tmp/reconcile.bin', 1024, $2) RETURNING id",
    )
    .bind(host_id)
    .bind(status)
    .fetch_one(db.pool())
    .await
    .expect("insert transfer")
}

/// (status, output, finished_at 文本) —— `finished_at::text` 避免引入 chrono 类型。
async fn job_state(db: &Db, id: Uuid) -> (String, Option<String>, Option<String>) {
    sqlx::query_as("SELECT status, output, finished_at::text FROM jobs WHERE id = $1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .expect("load job")
}

async fn transfer_state(db: &Db, id: Uuid) -> (String, Option<String>) {
    sqlx::query_as("SELECT status, finished_at::text FROM file_transfers WHERE id = $1")
        .bind(id)
        .fetch_one(db.pool())
        .await
        .expect("load transfer")
}

#[tokio::test]
async fn reconciles_orphans_and_spares_terminal_rows() {
    let _g = RECONCILE_GUARD.lock().await;
    let db = common::connect().await;
    let host = make_host(&db, "reconcile-alpha").await;

    let running = make_job(&db, host, "running", Some("partial output")).await;
    let queued = make_job(&db, host, "queued", None).await;
    let succeeded = make_job(&db, host, "succeeded", Some("done")).await;
    let timed_out = make_job(&db, host, "timed_out", None).await;

    let t_pending = make_transfer(&db, host, "pending").await;
    let t_transferring = make_transfer(&db, host, "transferring").await;
    let t_done = make_transfer(&db, host, "done").await;

    let report = reconcile_orphans(&db).await.expect("reconcile");
    // 报告计数是「本次调用对齐了多少条」；库与其它测试共享，jobs 计数可能被别的对账
    // 抢先（同库其它测试会造 running 行）→ 只对传输做下界断言（传输仅本文件造），
    // 真正的不变式断言在行状态上（见下）。
    assert!(
        report.transfers >= 2,
        "至少对齐本测试的 2 条传输：{report:?}"
    );

    for id in [running, queued] {
        let (status, output, finished_at) = job_state(&db, id).await;
        assert_eq!(
            status, "failed",
            "遗留 job 必须对齐为 failed（不新增状态枚举）"
        );
        assert!(finished_at.is_some(), "finished_at 必须落值");
        let output = output.expect("output 必须有说明");
        assert!(
            output.contains("server restarted"),
            "原因写进 output：{output}"
        );
    }

    // 原有输出必须保留（追加而非覆盖）
    let (_, running_output, _) = job_state(&db, running).await;
    assert!(
        running_output.unwrap().contains("partial output"),
        "已有 output 不能被覆盖"
    );

    // 已终态行不受影响
    assert_eq!(job_state(&db, succeeded).await.0, "succeeded");
    assert_eq!(job_state(&db, timed_out).await.0, "timed_out");
    assert_eq!(transfer_state(&db, t_done).await.0, "done");

    // 传输侧：非终态 → failed + finished_at 落值
    for id in [t_pending, t_transferring] {
        let (status, finished_at) = transfer_state(&db, id).await;
        assert_eq!(status, "failed");
        assert!(finished_at.is_some(), "传输 finished_at 必须落值");
    }
}

#[tokio::test]
async fn reconcile_is_idempotent() {
    let _g = RECONCILE_GUARD.lock().await;
    let db = common::connect().await;
    let host = make_host(&db, "reconcile-beta").await;
    let job = make_job(&db, host, "running", None).await;

    reconcile_orphans(&db).await.expect("first reconcile");
    let (status, _, first_finished) = job_state(&db, job).await;
    assert_eq!(status, "failed");

    reconcile_orphans(&db).await.expect("second reconcile");
    let (status_again, _, second_finished) = job_state(&db, job).await;
    assert_eq!(status_again, "failed");
    assert_eq!(
        first_finished, second_finished,
        "第二次对账不得改写已对齐的行（finished_at 不变）"
    );
}

/// 原因文案本身是控制台可见的产品文案，钉住关键要素。
#[test]
fn orphan_reason_text_is_specific() {
    assert!(ORPHAN_REASON.contains("server restarted"));
    assert!(ORPHAN_REASON.contains("failed"));
    assert!(
        ORPHAN_REASON.contains("非超时"),
        "必须与 sweeper 的 timed_out 语义区分：{ORPHAN_REASON}"
    );
}
