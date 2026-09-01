//! 进程管理 + 网络信息端点。

use crate::application::process_service::ProcessService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use helm_proto::pb::{NetInterface, ProcessInfo};
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

#[derive(Debug, Serialize)]
pub struct ProcessView {
    pub pid: i32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_bytes: u64,
}

impl From<ProcessInfo> for ProcessView {
    fn from(p: ProcessInfo) -> Self {
        Self {
            pid: p.pid,
            name: p.name,
            cpu_percent: p.cpu_percent,
            mem_bytes: p.mem_bytes,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct NetInterfaceView {
    pub name: String,
    pub addrs: Vec<String>,
}

impl From<NetInterface> for NetInterfaceView {
    fn from(i: NetInterface) -> Self {
        Self {
            name: i.name,
            addrs: i.addrs,
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
    Json(body): Json<KillBody>,
) -> Result<Json<Value>, Error> {
    let ok = service(&state).kill(&body.agent_id, body.pid).await?;
    Ok(Json(json!({ "pid": body.pid, "ok": ok })))
}

/// 采集网络信息：POST /api/v1/net/info
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
    Ok(Json(json!({
        "hostname": net.hostname,
        "interfaces": interfaces,
    })))
}
