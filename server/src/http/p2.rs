//! P2 运维能力端点：多主机批量执行 + 证据包一键收集 + NTFS 文件时间线。

use crate::application::exec_service::ExecService;
use crate::application::process_service::{IrScanResultView, ProcessService};
use crate::http::process::{NetInterfaceView, NetConnectionView, ProcessView, SysServiceView};
use crate::domain::Error;
use crate::http::AppState;
use axum::extract::State;
use axum::http::header;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

fn process_service(state: &AppState) -> ProcessService {
    ProcessService::new(state.conn_registry.clone(), state.query.clone())
}

// ---------------------------------------------------------------------------
// P2-9 多主机批量命令
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct BatchExecBody {
    pub agent_ids: Vec<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 批量命令下发：POST /api/v1/exec/batch
/// 逐 agent 建立 job（离线 agent 记 error），进度统一在任务页跟踪。
pub async fn batch_exec(
    State(state): State<AppState>,
    Json(body): Json<BatchExecBody>,
) -> std::result::Result<Json<Value>, Error> {
    if body.agent_ids.is_empty() {
        return Err(Error::InvalidArgument("agent_ids 为空".into()));
    }
    if body.command.trim().is_empty() {
        return Err(Error::InvalidArgument("command 为空".into()));
    }
    let svc = ExecService::new(state.db.clone(), state.registry.clone());
    let mut jobs = Vec::with_capacity(body.agent_ids.len());
    for agent_id in &body.agent_ids {
        match svc.exec(agent_id, &body.command, &body.args).await {
            Ok(job_id) => jobs.push(json!({ "agent_id": agent_id, "job_id": job_id })),
            Err(e) => jobs.push(json!({ "agent_id": agent_id, "error": e.to_string() })),
        }
    }
    Ok(Json(json!({
        "total": jobs.len(),
        "jobs": jobs,
    })))
}

// ---------------------------------------------------------------------------
// P2-10 证据包一键收集
// ---------------------------------------------------------------------------

/// 证据包：POST /api/v1/ir/evidence  → JSON 附件下载。
/// 汇集：主机档案 + 进程列表 + 网络状态 + 服务 + 自启动全景 + 注册表基线 + 系统日志 + 可疑文件。
pub async fn evidence(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> std::result::Result<axum::response::Response, Error> {
    let agent_id = body
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidArgument("缺少 agent_id".into()))?
        .to_string();

    let svc = process_service(&state);

    // 各数据源尽力采集，单项失败不阻断
    let mut pack = serde_json::Map::new();
    pack.insert(
        "generated_at".into(),
        json!(chrono::Utc::now().to_rfc3339()),
    );
    pack.insert("agent_id".into(), json!(agent_id));

    // 第一个数据源失败 = agent 不可达，直接报错（不要静默返回空证据包）
    let procs = svc
        .list(&agent_id)
        .await
        .map_err(|e| Error::NotConnected(format!("采集失败（agent 是否在线？）: {e}")))?;
    {
        let views: Vec<ProcessView> = procs.into_iter().map(ProcessView::from).collect();
        pack.insert("processes".into(), json!(views));
    }
    if let Ok(net) = svc.net(&agent_id).await {
        let ifaces: Vec<NetInterfaceView> = net.interfaces.into_iter().map(NetInterfaceView::from).collect();
        let conns: Vec<NetConnectionView> = net.connections.into_iter().map(NetConnectionView::from).collect();
        pack.insert("network".into(), json!({ "interfaces": ifaces, "connections": conns }));
    }
    if let Ok(services) = svc.sys_services(&agent_id).await {
        let views: Vec<SysServiceView> = services.services.into_iter().map(SysServiceView::from).collect();
        pack.insert("services".into(), json!(views));
    }
    let scan_types: [&[&str]; 4] = [
        &["autostart", "registry"],
        &["events"],
        &["suspicious_files"],
        &["accounts"],
    ];
    for types in scan_types {
        let types_v: Vec<String> = types.iter().map(|s| s.to_string()).collect();
        let key = types.join("+");
        if let Ok(r) = svc.ir_scan(&agent_id, &types_v).await {
            let IrScanResultView { findings, error } = r;
            pack.insert(format!("ir_{key}"), json!(findings));
            if let Some(e) = error {
                pack.insert(format!("ir_{key}_error"), json!(e));
            }
        }
    }

    // 主机档案
    let agents = crate::store::agent_repo::AgentRepo::new(state.db.clone())
        .list_all()
        .await
        .unwrap_or_default();
    if let Some(a) = agents.iter().find(|a| a.id == agent_id) {
        pack.insert("agent".into(), json!(a));
    }

    let body_bytes = serde_json::to_vec_pretty(&Value::Object(pack))
        .map_err(|e| Error::Internal(format!("证据包序列化失败: {e}")))?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let filename = format!("evidence-{agent_id}-{ts}.json");

    Ok(axum::response::Response::builder()
        .status(200)
        .header(header::CONTENT_TYPE, "application/json; charset=utf-8")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )
        .body(axum::body::Body::from(body_bytes))
        .unwrap())
}

// ---------------------------------------------------------------------------
// P2-12 NTFS USN 文件时间线
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct FsTimelineBody {
    pub agent_id: String,
    /// 盘符字母（默认 C）
    #[serde(default = "default_drive")]
    pub drive: String,
    /// 最近 N 小时（默认 24）
    #[serde(default)]
    pub since_hours: u32,
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub keyword: String,
}
fn default_drive() -> String {
    "C".into()
}
fn default_limit() -> u32 {
    5000
}

/// 文件时间线：POST /api/v1/ir/fs-timeline
pub async fn fs_timeline(
    State(state): State<AppState>,
    Json(body): Json<FsTimelineBody>,
) -> std::result::Result<Json<Value>, Error> {
    let r = process_service(&state)
        .fs_timeline(&body.agent_id, &body.drive, body.since_hours, body.limit, &body.keyword)
        .await?;
    Ok(Json(json!({
        "drive": r.drive,
        "entries": r
            .entries
            .iter()
            .map(|e| json!({
                "name": e.name,
                "frn": e.frn,
                "parentFrn": e.parent_frn,
                "tsUnix": e.ts_unix,
                "reason": e.reason,
            }))
            .collect::<Vec<_>>(),
        "totalScanned": r.total_scanned,
        "truncated": r.truncated,
        "error": r.error,
    })))
}
