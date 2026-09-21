//! 进程管理 + 网络信息 + 系统服务端点。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::application::process_service::ProcessService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Extension, State};
use helm_proto::pb::{NetConnection, NetInterface, ProcessInfo, SysServiceEntry};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct AgentBody {
    pub agent_id: String,
}

#[derive(Debug, Deserialize)]
pub struct KillBody {
    pub agent_id: String,
    pub pid: i32,
}

/// 进程视图（对标 Process Hacker：CPU/内存/属主/父进程/命令行）。
#[derive(Debug, Serialize)]
pub struct ProcessView {
    pub pid: i32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
    pub virt_mem_bytes: u64,
    pub parent_pid: i32,
    pub status: String,
    pub user: String,
    pub start_time_unix: u64,
    pub exe_path: String,
    pub cmd: String,
}

impl From<ProcessInfo> for ProcessView {
    fn from(p: ProcessInfo) -> Self {
        Self {
            pid: p.pid,
            name: p.name,
            cpu_percent: p.cpu_percent,
            mem_bytes: p.mem_bytes,
            virt_mem_bytes: p.virt_mem_bytes,
            parent_pid: p.parent_pid,
            status: p.status,
            user: p.user,
            start_time_unix: p.start_time_unix,
            exe_path: p.exe_path,
            cmd: p.cmd,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NetInterfaceView {
    pub name: String,
    pub addrs: Vec<String>,
    pub mac: String,
    pub status: String,
    pub gateway: String,
    pub kind: String,
}

impl From<NetInterface> for NetInterfaceView {
    fn from(i: NetInterface) -> Self {
        Self {
            name: i.name,
            addrs: i.addrs,
            mac: i.mac,
            status: i.status,
            gateway: i.gateway,
            kind: i.kind,
        }
    }
}

/// 网络连接视图（netstat/ss 视角的 TCP/UDP 会话表）。
#[derive(Debug, Serialize)]
pub struct NetConnectionView {
    pub protocol: String,
    pub local: String,
    pub remote: String,
    pub state: String,
    pub pid: i32,
    pub process_name: String,
}

impl From<NetConnection> for NetConnectionView {
    fn from(c: NetConnection) -> Self {
        Self {
            protocol: c.protocol,
            local: c.local,
            remote: c.remote,
            state: c.state,
            pid: c.pid,
            process_name: c.process_name,
        }
    }
}

/// 系统服务视图。
#[derive(Debug, Serialize)]
pub struct SysServiceView {
    pub name: String,
    pub display_name: String,
    pub status: String,
    pub start_type: String,
    pub pid: i32,
    pub description: String,
    /// systemd UnitFileState 原文（enabled/disabled/static/masked…）；非 systemd 为空。
    pub enabled_state: String,
    /// 进入当前状态的时刻（unix 秒；0 = 未知）。
    pub since_unix: i64,
    /// systemd 单元文件路径；非 systemd 为空。
    pub unit_file: String,
}

impl From<SysServiceEntry> for SysServiceView {
    fn from(s: SysServiceEntry) -> Self {
        Self {
            name: s.name,
            display_name: s.display_name,
            status: s.status,
            start_type: s.start_type,
            pid: s.pid,
            description: s.description,
            enabled_state: s.enabled_state,
            since_unix: s.since_unix as i64,
            unit_file: s.unit_file,
        }
    }
}

fn service(state: &AppState) -> ProcessService {
    ProcessService::new(state.registry.clone(), state.query.clone())
}

/// 列出进程：POST /api/v1/processes/list
pub async fn list_processes(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Result<Json<Value>, Error> {
    let procs: Vec<ProcessView> = service(&state)
        .list(&body.agent_id)
        .await?
        .into_iter()
        .map(ProcessView::from)
        .collect();
    Ok(Json(json!({ "processes": procs })))
}

/// 终止进程：POST /api/v1/processes/kill
pub async fn kill_process(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<KillBody>,
) -> Result<Json<Value>, Error> {
    let ok = service(&state).kill(&body.agent_id, body.pid).await?;
    AuditService::new(state.db.clone())
        .record_best_effort(
            &claims.sub,
            "process_kill",
            &body.agent_id,
            json!({ "pid": body.pid, "ok": ok }),
        )
        .await;
    Ok(Json(json!({ "pid": body.pid, "ok": ok })))
}

/// 采集网络信息（接口 + 连接表）：POST /api/v1/net/info
pub async fn net_info(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Result<Json<Value>, Error> {
    let net = service(&state).net(&body.agent_id).await?;
    let interfaces: Vec<NetInterfaceView> = net
        .interfaces
        .into_iter()
        .map(NetInterfaceView::from)
        .collect();
    let connections: Vec<NetConnectionView> = net
        .connections
        .into_iter()
        .map(NetConnectionView::from)
        .collect();
    Ok(Json(json!({
        "hostname": net.hostname,
        "interfaces": interfaces,
        "connections": connections,
    })))
}

/// 系统服务操作请求。
#[derive(Debug, Deserialize)]
pub struct SysServiceActionBody {
    pub agent_id: String,
    /// 服务标识（Windows 服务名 / systemd unit / launchctl label）
    pub name: String,
    /// start | stop | restart
    pub action: String,
}

/// 枚举系统服务：POST /api/v1/sys-services/list
pub async fn list_sys_services(
    State(state): State<AppState>,
    Json(body): Json<AgentBody>,
) -> Result<Json<Value>, Error> {
    let result = service(&state).sys_services(&body.agent_id).await?;
    let services: Vec<SysServiceView> = result
        .services
        .into_iter()
        .map(SysServiceView::from)
        .collect();
    Ok(Json(json!({ "services": services, "error": result.error })))
}

/// 系统服务操作：POST /api/v1/sys-services/action
pub async fn sys_service_action(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<SysServiceActionBody>,
) -> Result<Json<Value>, Error> {
    let (ok, error) = service(&state)
        .sys_service_action(&body.agent_id, &body.name, &body.action)
        .await?;
    AuditService::new(state.db.clone())
        .record_best_effort(
            &claims.sub,
            "sys_service_action",
            &body.agent_id,
            json!({ "name": body.name, "action": body.action, "ok": ok }),
        )
        .await;
    Ok(Json(
        json!({ "name": body.name, "action": body.action, "ok": ok, "error": error }),
    ))
}
