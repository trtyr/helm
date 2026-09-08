//! IR 快照（基线对比）与 VirusTotal 查杀缓存仓储。

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

/// VT 查杀缓存行。
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct IrVtCacheRow {
    pub sha256: String,
    pub positives: i32,
    pub total: i32,
    pub checked_at: DateTime<Utc>,
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
    let r = sqlx::query("DELETE FROM ir_snapshots WHERE id = $1").bind(id).execute(pool).await?;
    Ok(r.rows_affected())
}

/// 读取 VT 缓存（7 天过期由调用方判断）。
pub async fn get_vt_cache(pool: &PgPool, sha256: &str) -> sqlx::Result<Option<IrVtCacheRow>> {
    sqlx::query_as(
        "SELECT sha256, positives, total, checked_at FROM ir_vt_cache WHERE sha256 = $1",
    )
    .bind(sha256)
    .fetch_optional(pool)
    .await
}

pub async fn upsert_vt_cache(
    pool: &PgPool,
    sha256: &str,
    positives: i32,
    total: i32,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO ir_vt_cache (sha256, positives, total) VALUES ($1, $2, $3)
         ON CONFLICT (sha256) DO UPDATE SET positives = EXCLUDED.positives, total = EXCLUDED.total, checked_at = now()",
    )
    .bind(sha256)
    .bind(positives)
    .bind(total)
    .execute(pool)
    .await?;
    Ok(())
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
pub async fn get_page_cache(pool: &PgPool, agent_id: &str, kind: &str) -> sqlx::Result<Option<IrPageCacheRow>> {
    sqlx::query_as(
        "SELECT agent_id, kind, findings, entry_count, created_at FROM ir_page_cache WHERE agent_id = $1 AND kind = $2",
    )
    .bind(agent_id)
    .bind(kind)
    .fetch_optional(pool)
    .await
}
