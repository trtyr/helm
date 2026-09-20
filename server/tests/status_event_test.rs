//! 状态事件集成测试（P003 T1）：insert / list_paged / count_filtered 的
//! filter 镜像一致性（count 与 list 同过滤范围必须相等——审计不变式）。

mod common;

use helm_server::store::status_event_repo::StatusEventRepo;

#[tokio::test]
async fn count_and_list_mirror_filters() {
    let db = common::connect().await;
    let repo = StatusEventRepo::new(db);

    // 唯一前缀隔离：并行测试库共享，不与其他 fixture 碰撞
    repo.insert("se-alpha", "online", "se_registered", "")
        .await
        .unwrap();
    repo.insert(
        "se-alpha",
        "offline",
        "se_transport_error",
        "h2 connection reset",
    )
    .await
    .unwrap();
    repo.insert("se-beta", "online", "se_registered", "")
        .await
        .unwrap();

    // 全量可见（不假设表为空——并行库共享）
    let all = repo
        .list_paged(None, None, None, None, None, 100, 0)
        .await
        .unwrap();
    let total = repo.count_filtered(None, None, None, None).await.unwrap();
    assert!(all.len() >= 3, "至少 3 条 fixture: {}", all.len());
    assert_eq!(all.len() as i64, total, "list 长度必须等于 count（全量）");

    // host 过滤：count 与 list 长度镜像
    let list_a = repo
        .list_paged(None, Some("se-alpha"), None, None, None, 100, 0)
        .await
        .unwrap();
    let count_a = repo
        .count_filtered(None, Some("se-alpha"), None, None)
        .await
        .unwrap();
    assert_eq!(list_a.len() as i64, count_a, "host 过滤 count/list 镜像");
    assert_eq!(count_a, 2);

    // q 过滤：命中 reason（count/list 镜像 + 内容断言）
    let list_q = repo
        .list_paged(Some("se_transport"), None, None, None, None, 100, 0)
        .await
        .unwrap();
    let count_q = repo
        .count_filtered(Some("se_transport"), None, None, None)
        .await
        .unwrap();
    assert_eq!(list_q.len() as i64, count_q, "q 过滤 count/list 镜像");
    assert_eq!(count_q, 1);
    assert_eq!(list_q[0].reason, "se_transport_error");
    assert_eq!(list_q[0].detail, "h2 connection reset");

    // 默认排序 created_at DESC：最后插入的 se-beta online 在前
    let newest = &all[0];
    assert_eq!(newest.host_id, "se-beta");
    assert_eq!(newest.event, "online");
}
