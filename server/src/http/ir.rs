//! 应急响应端点：IR 基线扫描 + 进程内存字符串扫描。

use crate::application::process_service::{IrScanResultView, MemScanResultView, ProcessService};
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

fn service(state: &AppState) -> ProcessService {
    ProcessService::new(state.conn_registry.clone(), state.query.clone())
}

/// IR 扫描请求。
#[derive(Debug, Deserialize)]
pub struct IrScanBody {
    pub agent_id: String,
    /// accounts | autostart | events | suspicious_files（空 = 全部）
    #[serde(default)]
    pub types: Vec<String>,
}

/// 应急扫描：POST /api/v1/ir/scan
pub async fn ir_scan(
    State(state): State<AppState>,
    Json(body): Json<IrScanBody>,
) -> Result<Json<Value>, Error> {
    for t in &body.types {
        if !matches!(
            t.as_str(),
            "accounts" | "autostart" | "registry" | "events" | "suspicious_files"
        ) {
            return Err(Error::InvalidArgument(format!("未知扫描类型: {t}")));
        }
    }
    let result: IrScanResultView = service(&state).ir_scan(&body.agent_id, &body.types).await?;

    // 扫描结果写入页面缓存（key = 排序后的类型组合，如 "autostart,registry"）
    let mut sorted_types = body.types.clone();
    sorted_types.sort();
    let kind = sorted_types.join(",");
    let findings_json = serde_json::json!(result.findings);
    let count = findings_json.as_array().map(|a| a.len()).unwrap_or(0) as i32;
    // 页面缓存写失败只影响后续读取性能，不影响本次结果；但 DB 异常必须留痕
    if let Err(e) = crate::store::ir_repo::upsert_page_cache(
        state.db.pool(),
        &body.agent_id,
        &kind,
        &findings_json,
        count,
    )
    .await
    {
        tracing::warn!(agent_id = %body.agent_id, kinds = %kind, error = %e, "ir page cache upsert failed");
    }

    Ok(Json(json!({
        "findings": result.findings,
        "error": result.error,
    })))
}

/// 页面缓存查询：GET /api/v1/ir/cache?agent_id=&types=
pub async fn get_cache(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, Error> {
    let Some(agent_id) = q.get("agent_id") else {
        return Err(Error::InvalidArgument("缺少 agent_id".into()));
    };
    let Some(types) = q.get("types") else {
        return Err(Error::InvalidArgument("缺少 types".into()));
    };
    let mut parts: Vec<&str> = types
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    parts.sort_unstable();
    let kind = parts.join(",");
    match crate::store::ir_repo::get_page_cache(state.db.pool(), agent_id, &kind).await {
        Ok(Some(row)) => Ok(Json(json!({
            "findings": row.findings,
            "entryCount": row.entry_count,
            "createdAt": row.created_at,
        }))),
        Ok(None) => Ok(Json(json!({ "findings": null }))),
        Err(e) => Err(Error::Internal(format!("查询缓存失败: {e}"))),
    }
}

/// 内存扫描请求。
#[derive(Debug, Deserialize)]
pub struct MemScanBody {
    pub agent_id: String,
    pub pid: i32,
    #[serde(default)]
    pub min_len: u32,
    #[serde(default)]
    pub keyword: String,
}

/// 进程内存字符串扫描：POST /api/v1/ir/memscan
pub async fn mem_scan(
    State(state): State<AppState>,
    Json(body): Json<MemScanBody>,
) -> Result<Json<Value>, Error> {
    // 体检新提项（T5）：pid 入口校验——只接受 0（全进程）或正数，
    // 负数/异常值在入口拒绝，不下发给 agent（避免把不可信输入带到被控端）
    if body.pid < 0 {
        return Err(Error::InvalidArgument(
            "pid must be >= 0 (0 = all processes)".into(),
        ));
    }
    let result: MemScanResultView = service(&state)
        .mem_scan(&body.agent_id, body.pid, body.min_len, &body.keyword)
        .await?;
    Ok(Json(json!({
        "pid": result.pid,
        "matches": result.matches,
        "scanned_bytes": result.scanned_bytes,
        "truncated": result.truncated,
        "error": result.error,
        "pidsTotal": result.pids_total,
        "pidsScanned": result.pids_scanned,
    })))
}
