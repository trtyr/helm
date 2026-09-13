//! IR 操作端点：启动项操作（禁用/启用/删除）+ 快照基线对比 + VirusTotal 查杀 + 文件元数据。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::application::process_service::{IrScanResultView, ProcessService};
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};

fn service(state: &AppState) -> ProcessService {
    ProcessService::new(state.conn_registry.clone(), state.query.clone())
}

// ---------------------------------------------------------------------------
// 启动项操作（禁用/启用/删除）
// ---------------------------------------------------------------------------

/// 启动项操作请求。
#[derive(Debug, Deserialize)]
pub struct AutorunsActionBody {
    pub agent_id: String,
    /// disable | enable | delete
    pub action: String,
    /// 扫描结果中的 opKey
    pub key: String,
}

/// 启动项操作：POST /api/v1/ir/autorun-action
pub async fn autorun_action(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<AutorunsActionBody>,
) -> Result<Json<Value>, Error> {
    if !matches!(body.action.as_str(), "disable" | "enable" | "delete") {
        return Err(Error::InvalidArgument(format!("未知操作: {}", body.action)));
    }
    let (ok, error) = service(&state)
        .autoruns_action(&body.agent_id, &body.action, &body.key)
        .await?;
    let _ = AuditService::new(state.db.clone())
        .record(
            &claims.sub,
            "autorun_action",
            &body.agent_id,
            json!({ "action": body.action, "key": body.key, "ok": ok }),
        )
        .await;
    Ok(Json(json!({ "ok": ok, "error": error })))
}

// ---------------------------------------------------------------------------
// 文件元数据（SHA256，供 VT 查杀）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct FileMetaBody {
    pub agent_id: String,
    pub path: String,
}

/// 文件元数据：POST /api/v1/ir/file-meta
pub async fn file_meta(
    State(state): State<AppState>,
    Json(body): Json<FileMetaBody>,
) -> Result<Json<Value>, Error> {
    let r = service(&state)
        .file_meta(&body.agent_id, &body.path)
        .await?;
    Ok(Json(json!({
        "path": r.path,
        "sha256": r.sha256,
        "size": r.size,
        "mtime": r.mtime,
        "error": r.error,
    })))
}

// ---------------------------------------------------------------------------
// 基线快照（保存 / 列表 / 详情 / 删除 / 对比）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SnapshotCreateBody {
    pub agent_id: String,
    #[serde(default)]
    pub label: String,
}

/// 保存当前扫描为基线快照：POST /api/v1/ir/snapshots
pub async fn create_snapshot(
    State(state): State<AppState>,
    Json(body): Json<SnapshotCreateBody>,
) -> Result<Json<Value>, Error> {
    let result: IrScanResultView = service(&state)
        .ir_scan(
            &body.agent_id,
            &["autostart".to_string(), "registry".to_string()],
        )
        .await?;
    let findings = json!(result.findings);
    let count = findings.as_array().map(|a| a.len()).unwrap_or(0) as i32;
    let id = crate::store::ir_repo::insert_snapshot(
        state.db.pool(),
        &body.agent_id,
        &body.label,
        &findings,
        count,
    )
    .await
    .map_err(|e| Error::Internal(format!("保存快照失败: {e}")))?;
    Ok(Json(json!({ "id": id, "entry_count": count })))
}

/// 快照列表：GET /api/v1/ir/snapshots?agent_id=
pub async fn list_snapshots(
    State(state): State<AppState>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, Error> {
    let Some(agent_id) = q.get("agent_id") else {
        return Err(Error::InvalidArgument("缺少 agent_id".into()));
    };
    let rows = crate::store::ir_repo::list_snapshots(state.db.pool(), agent_id)
        .await
        .map_err(|e| Error::Internal(format!("查询快照失败: {e}")))?;
    Ok(Json(json!({ "snapshots": rows })))
}

/// 快照详情：GET /api/v1/ir/snapshots/{id}
pub async fn get_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let uuid = parse_uuid(&id)?;
    let snap = crate::store::ir_repo::get_snapshot(state.db.pool(), uuid)
        .await
        .map_err(|e| Error::Internal(format!("查询快照失败: {e}")))?
        .ok_or_else(|| Error::InvalidArgument("快照不存在".into()))?;
    Ok(Json(json!({ "snapshot": snap })))
}

/// 删除快照：DELETE /api/v1/ir/snapshots/{id}
pub async fn delete_snapshot(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, Error> {
    let uuid = parse_uuid(&id)?;
    let n = crate::store::ir_repo::delete_snapshot(state.db.pool(), uuid)
        .await
        .map_err(|e| Error::Internal(format!("删除快照失败: {e}")))?;
    Ok(Json(json!({ "deleted": n })))
}

#[derive(Debug, Deserialize)]
pub struct SnapshotCompareBody {
    /// 基线快照 id
    pub base_id: String,
    /// 对比目标快照 id（与 agent_id 二选一）
    #[serde(default)]
    pub target_id: String,
    /// 给了 agent_id 则现场重扫一次作为对比目标（不入库）
    #[serde(default)]
    pub agent_id: String,
}

/// 基线对比：POST /api/v1/ir/snapshots/compare
/// 返回 added（目标有、基线无）与 removed（基线有、目标无）。
pub async fn compare_snapshots(
    State(state): State<AppState>,
    Json(body): Json<SnapshotCompareBody>,
) -> Result<Json<Value>, Error> {
    let base_uuid = parse_uuid(&body.base_id)?;
    let base = crate::store::ir_repo::get_snapshot(state.db.pool(), base_uuid)
        .await
        .map_err(|e| Error::Internal(format!("查询基线失败: {e}")))?
        .ok_or_else(|| Error::InvalidArgument("基线快照不存在".into()))?;

    // 对比目标：另一份快照 或 现场重扫
    let (target_findings, target_label) = if !body.target_id.is_empty() {
        let t_uuid = parse_uuid(&body.target_id)?;
        let t = crate::store::ir_repo::get_snapshot(state.db.pool(), t_uuid)
            .await
            .map_err(|e| Error::Internal(format!("查询目标失败: {e}")))?
            .ok_or_else(|| Error::InvalidArgument("目标快照不存在".into()))?;
        (t.findings, t.created_at.to_string())
    } else if !body.agent_id.is_empty() {
        let result: IrScanResultView = service(&state)
            .ir_scan(
                &body.agent_id,
                &["autostart".to_string(), "registry".to_string()],
            )
            .await?;
        (json!(result.findings), "live".to_string())
    } else {
        return Err(Error::InvalidArgument(
            "target_id 与 agent_id 至少给一个".into(),
        ));
    };

    let (added, removed) = diff_findings(base.findings.clone(), target_findings.clone());
    Ok(Json(json!({
        "base": { "id": base.id, "label": base.label, "created_at": base.created_at, "entry_count": base.entry_count },
        "target": target_label,
        "added": added,
        "removed": removed,
    })))
}

/// 条目身份键：优先 opKey（操作地址唯一），回退 category+name+path。
fn finding_identity(f: &Value) -> String {
    let s = |v: &Value| v.as_str().unwrap_or("").to_string();
    let op = s(&f["opKey"]);
    if !op.is_empty() {
        return format!("op:{op}");
    }
    format!(
        "n:{}|{}|{}",
        s(&f["category"]),
        s(&f["name"]),
        s(&f["path"])
    )
}

fn diff_findings(base: Value, target: Value) -> (Vec<Value>, Vec<Value>) {
    let to_map = |v: Value| -> std::collections::HashMap<String, Value> {
        let empty = Vec::new();
        let arr = v.as_array().unwrap_or(&empty);
        arr.iter()
            .map(|f| (finding_identity(f), f.clone()))
            .collect()
    };
    let base_map = to_map(base);
    let target_map = to_map(target);
    let added: Vec<Value> = target_map
        .iter()
        .filter(|(k, _)| !base_map.contains_key(*k))
        .map(|(_, v)| v.clone())
        .collect();
    let removed: Vec<Value> = base_map
        .iter()
        .filter(|(k, _)| !target_map.contains_key(*k))
        .map(|(_, v)| v.clone())
        .collect();
    (added, removed)
}

fn parse_uuid(s: &str) -> Result<sqlx::types::Uuid, Error> {
    sqlx::types::Uuid::parse_str(s).map_err(|_| Error::InvalidArgument(format!("无效快照 id: {s}")))
}

// ---------------------------------------------------------------------------
// VirusTotal 查杀（按 SHA256，服务端缓存）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct VtLookupBody {
    pub sha256: String,
}

/// VT 查询：POST /api/v1/ir/vt
/// 需要配置 HELM_VT_API_KEY；结果缓存于 ir_vt_cache（7 天内复用）。
pub async fn vt_lookup(
    State(state): State<AppState>,
    Json(body): Json<VtLookupBody>,
) -> Result<Json<Value>, Error> {
    let sha = body.sha256.trim().to_ascii_lowercase();
    if sha.len() != 64 || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(Error::InvalidArgument("需要 64 位十六进制 SHA256".into()));
    }
    let permalink = format!("https://www.virustotal.com/gui/file/{sha}");

    // 缓存命中（7 天内）
    if let Ok(Some(c)) = crate::store::ir_repo::get_vt_cache(state.db.pool(), &sha).await {
        let age = chrono::Utc::now().signed_duration_since(c.checked_at);
        if age < chrono::Duration::days(7) {
            return Ok(Json(json!({
                "sha256": sha, "positives": c.positives, "total": c.total,
                "permalink": permalink, "cached": true,
            })));
        }
    }

    let key = &state.vt_api_key;
    if key.is_empty() {
        return Err(Error::InvalidArgument(
            "未配置 VirusTotal API key（HELM_VT_API_KEY 环境变量）".into(),
        ));
    }

    let client = reqwest::Client::new();
    let url = format!("https://www.virustotal.com/api/v3/files/{sha}");
    let resp = client
        .get(&url)
        .header("x-apikey", key.as_str())
        .timeout(std::time::Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| Error::Internal(format!("VT 请求失败: {e}")))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        // VT 未收录：positives=-1 表示未知样本
        let _ = crate::store::ir_repo::upsert_vt_cache(state.db.pool(), &sha, -1, 0).await;
        return Ok(Json(json!({
            "sha256": sha, "positives": -1, "total": 0,
            "permalink": permalink, "cached": false,
        })));
    }
    if resp.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Error::InvalidArgument(
            "VT 配额超限（公共 API 每分钟 4 次），稍后再试".into(),
        ));
    }
    if !resp.status().is_success() {
        return Err(Error::Internal(format!("VT 返回状态 {}", resp.status())));
    }
    let v: Value = resp
        .json()
        .await
        .map_err(|e| Error::Internal(format!("VT 解析失败: {e}")))?;
    let stats = &v["data"]["attributes"]["last_analysis_stats"];
    let malicious = stats["malicious"].as_i64().unwrap_or(0);
    let suspicious = stats["suspicious"].as_i64().unwrap_or(0);
    let undetected = stats["undetected"].as_i64().unwrap_or(0);
    let total = malicious + suspicious + undetected;
    let positives = malicious + suspicious;
    let _ = crate::store::ir_repo::upsert_vt_cache(
        state.db.pool(),
        &sha,
        positives as i32,
        total as i32,
    )
    .await;
    Ok(Json(json!({
        "sha256": sha, "positives": positives, "total": total,
        "permalink": permalink, "cached": false,
    })))
}

// ---------------------------------------------------------------------------
// 流式内存扫描（WS 增量推送）
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct MemScanStreamBody {
    pub agent_id: String,
    /// 客户端生成（UUID）——先订阅 WS 再启动扫描，避免早期批次丢失
    pub scan_id: String,
    /// 0 = 全部进程
    pub pid: i32,
    #[serde(default)]
    pub min_len: u32,
    #[serde(default)]
    pub keyword: String,
}

/// 启动流式内存扫描：POST /api/v1/ir/memscan/stream
/// scanId 由客户端生成并先行订阅 WS /api/v1/ir/memscan/{scanId}/stream。
pub async fn memscan_stream_start(
    State(state): State<AppState>,
    Json(body): Json<MemScanStreamBody>,
) -> Result<Json<Value>, Error> {
    // scanId 只作频道键，校验 UUID 防垃圾 key
    let scan_id = uuid::Uuid::parse_str(&body.scan_id)
        .map_err(|_| Error::InvalidArgument("scan_id 需为 UUID".into()))?
        .to_string();
    service(&state)
        .mem_scan_start(
            &body.agent_id,
            &scan_id,
            body.pid,
            body.min_len,
            &body.keyword,
        )
        .await?;
    Ok(Json(json!({ "scanId": scan_id })))
}
