//! Agent 生命周期端点：列出 / 下线注销 / 卸载。

use crate::application::agent_lifecycle_service::AgentLifecycleService;
use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::agent_repo::{AgentRepo, AgentRow};
use crate::store::host_repo::HostRepo;
use axum::Json;
use axum::extract::{Extension, Path, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// 卸载指令参数。
#[derive(Debug, Deserialize)]
pub struct UninstallBody {
    /// 是否删除自身二进制文件（默认 true = 完整卸载）。
    #[serde(default = "default_true")]
    pub remove_binary: bool,
}

/// 设置标签参数。
#[derive(Debug, Deserialize)]
pub struct SetTagsBody {
    pub tags: Vec<String>,
}

/// agent 详情视图：附在线状态。
#[derive(Debug, Serialize)]
pub struct AgentDetail {
    #[serde(flatten)]
    pub agent: AgentRow,
    pub online: bool,
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
    let delivered = AgentLifecycleService::new(state.db, state.registry)
        .uninstall(&agent_id, body.remove_binary)
        .await?;
    Ok(Json(json!({
        "ok": true,
        "delivered": delivered,
        "deferred": !delivered,
        "message": if delivered { String::new() } else { "agent 离线：已标记挂起，重连瞬间将自动下线".to_string() },
    })))
}

/// 详情：GET /api/v1/agents/{id}
pub async fn get_agent(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
) -> Result<Json<Value>, Error> {
    let agent = AgentRepo::new(state.db)
        .get(&agent_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;
    let online = state.registry.is_online(&agent_id).await;
    Ok(Json(json!({ "agent": AgentDetail { agent, online } })))
}

/// 更新 agent 所属 host 的标签：PUT /api/v1/agents/{id}/tags
pub async fn update_agent_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(agent_id): Path<String>,
    Json(body): Json<SetTagsBody>,
) -> Result<Json<Value>, Error> {
    let host_id = AgentRepo::new(state.db.clone())
        .get_host_id(&agent_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;
    let host = HostRepo::new(state.db.clone())
        .set_tags(host_id, &body.tags)
        .await?
        .ok_or_else(|| Error::NotFound(format!("host: {host_id}")))?;
    AuditService::new(state.db)
        .record_best_effort(&claims.sub, "agent_tags", &agent_id, json!({}))
        .await;
    Ok(Json(json!({ "host": host })))
}
