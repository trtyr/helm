//! IR 快照（基线对比）仓储。

use sqlx::PgPool;

/// 快照元数据（不含 findings 全文）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct IrSnapshotMeta {
    pub id: Uuid,
    pub agent_id: String,
    pub label: String,
    pub entry_count: i32,
    pub created_at: DateTime<Upt>,
}

// 别名简写，避免直接暴露 chrono 类型名
use chrono::{DateTime, Utc};
type Upt = Utc;
type Uuid = sqlx::types::Uuid;

/// 快照全文（含 findings）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct IrSnapshotFull {
    pub id: Uuid,
    pub agent_id: String,
    pub label: String,
    pub findings: serde_json::Value,
    pub entry_count: i32,
    pub created_at: DateTime<Utc>,
}

pub async fn insert_snapshot(
    pool: &PgPool,
    agent_id: &str,
    label: &str,
    findings: &serde_json::Value,
    entry_count: i32,
) -> sqlx::Result<Uuid> {
    let rec: (Uuid,) = sqlx::query_as(
        "INSERT INTO ir_snapshots (agent_id, label, findings, entry_count) VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(agent_id)
    .bind(label)
    .bind(findings)
    .bind(entry_count)
    .fetch_one(pool)
    .await?;
    Ok(rec.0)
}

pub async fn list_snapshots(pool: &PgPool, agent_id: &str) -> sqlx::Result<Vec<IrSnapshotMeta>> {
    sqlx::query_as(
        "SELECT id, agent_id, label, entry_count, created_at FROM ir_snapshots WHERE agent_id = $1 ORDER BY created_at DESC",
    )
    .bind(agent_id)
    .fetch_all(pool)
    .await
}

pub async fn get_snapshot(pool: &PgPool, id: Uuid) -> sqlx::Result<Option<IrSnapshotFull>> {
    sqlx::query_as(
        "SELECT id, agent_id, label, findings, entry_count, created_at FROM ir_snapshots WHERE id = $1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn delete_snapshot(pool: &PgPool, id: Uuid) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM ir_snapshots WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// 保留策略（T009）：每主机只保留最近 `keep_per_agent` 条快照（超额删），返回删除行数。
///
/// 快照是取证资产（基线对比 / 差异取证），不按时间一刀切——按「每主机条数」封顶：
/// 既不丢最近的证据链，又给磁盘设了上限。`keep_per_agent <= 0` 视为不清理。
pub async fn delete_excess_snapshots(pool: &PgPool, keep_per_agent: i64) -> sqlx::Result<u64> {
    if keep_per_agent <= 0 {
        return Ok(0);
    }
    let r = sqlx::query(
        "DELETE FROM ir_snapshots s
          USING (
            SELECT id FROM (
              SELECT id,
                     row_number() OVER (PARTITION BY agent_id ORDER BY created_at DESC) AS rn
                FROM ir_snapshots
            ) ranked
             WHERE ranked.rn > $1
          ) doomed
          WHERE s.id = doomed.id",
    )
    .bind(keep_per_agent)
    .execute(pool)
    .await?;
    Ok(r.rows_affected())
}

/// 保留策略（T009）：删除超过 `cutoff` 未再刷新的页面缓存行，返回删除行数。
///
/// `ir_page_cache` 以 `(agent_id, kind)` 为主键、写入即 `created_at = now()`（upsert），
/// 因此「超期」= 该主机该页面已很久没被扫描刷新。行数本身有界（主机数 × kind 数），
/// 这里治的是**陈旧内容**与「已注销主机留下的死缓存」。
pub async fn delete_stale_page_cache(pool: &PgPool, cutoff: DateTime<Utc>) -> sqlx::Result<u64> {
    let r = sqlx::query("DELETE FROM ir_page_cache WHERE created_at < $1")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

/// 页面级缓存行（最后一次扫描结果）。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct IrPageCacheRow {
    pub agent_id: String,
    pub kind: String,
    pub findings: serde_json::Value,
    pub entry_count: i32,
    pub created_at: DateTime<Utc>,
}

/// 写入/更新页面缓存。
pub async fn upsert_page_cache(
    pool: &PgPool,
    agent_id: &str,
    kind: &str,
    findings: &serde_json::Value,
    entry_count: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ir_page_cache (agent_id, kind, findings, entry_count) VALUES ($1, $2, $3, $4)
         ON CONFLICT (agent_id, kind) DO UPDATE SET findings = EXCLUDED.findings, entry_count = EXCLUDED.entry_count, created_at = now()",
    )
    .bind(agent_id)
    .bind(kind)
    .bind(findings)
    .bind(entry_count)
    .execute(pool)
    .await?;
    Ok(())
}

/// 读取页面缓存。
pub async fn get_page_cache(
    pool: &PgPool,
    agent_id: &str,
    kind: &str,
) -> sqlx::Result<Option<IrPageCacheRow>> {
    sqlx::query_as(
        "SELECT agent_id, kind, findings, entry_count, created_at FROM ir_page_cache WHERE agent_id = $1 AND kind = $2",
    )
    .bind(agent_id)
    .bind(kind)
    .fetch_optional(pool)
    .await
}
