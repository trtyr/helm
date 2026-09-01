//! Agent 生命周期端点：列出 / 下线注销 / 卸载。

use crate::application::agent_lifecycle_service::AgentLifecycleService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Path, State};
use serde::Deserialize;
use serde_json::{Value, json};

/// 卸载指令参数。
#[derive(Debug, Deserialize)]
pub struct UninstallBody {
    /// 是否删除自身二进制文件（默认 true = 完整卸载）。
    #[serde(default = "default_true")]
    pub remove_binary: bool,
}

fn default_true() -> bool {
    true
}

/// 列出 agent：GET /api/v1/agents
pub async fn list_agents(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let agents = AgentLifecycleService::new(state.db, state.registry)
        .list()
        .await?;
    Ok(Json(json!({ "agents": agents })))
}

/// 下线/注销：DELETE /api/v1/agents/{id}
pub async fn deregister_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, Error> {
    AgentLifecycleService::new(state.db, state.registry)
        .deregister(&agent_id)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

/// 卸载：POST /api/v1/agents/{id}/uninstall
pub async fn uninstall_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Json(body): Json<UninstallBody>,
) -> Result<Json<Value>, Error> {
    AgentLifecycleService::new(state.db, state.registry)
        .uninstall(&agent_id, body.remove_binary)
        .await?;
    Ok(Json(json!({ "ok": true })))
}
