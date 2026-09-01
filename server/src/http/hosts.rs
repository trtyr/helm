//! 主机查询与创建端点。

use crate::application::online_status::is_stale;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::agent_repo::AgentRepo;
use crate::store::host_repo::{HostRepo, HostRow, NewHost};
use axum::Json;
use axum::extract::State;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 创建主机参数。
#[derive(Debug, Deserialize)]
pub struct CreateHostBody {
    pub hostname: String,
    /// reverse（默认，Agent 主动连）| forward（Server 主动连）
    #[serde(default = "default_conn_mode")]
    pub conn_mode: String,
    /// forward 模式的拨号地址（如 100.80.65.64:50052）
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_conn_mode() -> String {
    "reverse".to_string()
}

/// 主机列表视图：附在线状态 + 最后心跳 + 心跳超时标记。
#[derive(Debug, Serialize)]
struct HostView {
    #[serde(flatten)]
    host: HostRow,
    online: bool,
    last_seen: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后心跳是否超时（从未心跳或超过阈值；forward 主机无心跳恒为 true）。
    stale: bool,
}

/// 列出主机：GET /api/v1/hosts（附在线状态 + 最后心跳 + 心跳超时标记）
pub async fn list_hosts(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let db = state.db.clone();
    let registry = state.registry.clone();
    let timeout = chrono::Duration::seconds(state.heartbeat_timeout_secs as i64);

    let hosts = HostRepo::new(db.clone()).list().await?;
    let agent_repo = AgentRepo::new(db);

    let mut views = Vec::with_capacity(hosts.len());
    for host in hosts {
        let agent_ids = agent_repo.list_agent_ids(host.id).await?;
        let online = registry.any_online(&agent_ids).await;
        let last_seen = agent_repo.last_heartbeat(host.id).await?;
        let stale = is_stale(last_seen, chrono::Utc::now(), timeout);
        views.push(HostView {
            host,
            online,
            last_seen,
            stale,
        });
    }

    Ok(Json(json!({ "hosts": views })))
}

/// 创建主机：POST /api/v1/hosts
pub async fn create_host(
    State(state): State<AppState>,
    Json(body): Json<CreateHostBody>,
) -> Result<Json<Value>, Error> {
    let conn_mode = if body.conn_mode == "forward" {
        "forward".to_string()
    } else {
        "reverse".to_string()
    };
    let host = HostRepo::new(state.db)
        .insert(&NewHost {
            hostname: body.hostname,
            // forward 模式下目标机细节未知，拨号成功后由 Agent 回报补全
            os: String::new(),
            arch: String::new(),
            platform: String::new(),
            tags: body.tags,
            conn_mode,
            addr: body.addr,
        })
        .await?;
    Ok(Json(json!({ "host": host })))
}
