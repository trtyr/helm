//! 主机查询与创建端点。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::application::online_status::is_stale;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::agent_repo::AgentRepo;
use crate::store::host_repo::{HostRepo, HostRow, NewHost};
use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

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

/// 主机列表视图：附在线状态 + 最后心跳 + 心跳超时标记 + 关联 Agent 标识/版本。
#[derive(Debug, Serialize)]
struct HostView {
    #[serde(flatten)]
    host: HostRow,
    online: bool,
    last_seen: Option<chrono::DateTime<chrono::Utc>>,
    /// 最后心跳是否超时（从未心跳或超过阈值；forward 主机无心跳恒为 true）。
    stale: bool,
    /// 关联 Agent（最近注册的一个；无则为 null）。
    agent_id: Option<String>,
    agent_version: Option<String>,
    /// 关联 Agent 是否以管理员/root 权限运行（无 agent 则 null）。
    agent_elevated: Option<bool>,
}

/// 列表查询参数（可选标签过滤 + 分页）。
#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

/// 更新主机参数。
#[derive(Debug, Deserialize)]
pub struct UpdateHostBody {
    pub hostname: String,
    #[serde(default = "default_conn_mode")]
    pub conn_mode: String,
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub os: String,
    #[serde(default)]
    pub arch: String,
    #[serde(default)]
    pub platform: String,
}

/// 设置标签参数。
#[derive(Debug, Deserialize)]
pub struct SetTagsBody {
    pub tags: Vec<String>,
}

/// 列出主机：GET /api/v1/hosts（附在线状态 + 最后心跳 + 心跳超时标记；支持 ?tag= 过滤 + ?page=&limit= 分页）
pub async fn list_hosts(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    let db = state.db.clone();
    let registry = state.registry.clone();
    let timeout = chrono::Duration::seconds(state.heartbeat_timeout_secs as i64);

    let hosts = match &q.tag {
        Some(tag) => HostRepo::new(db.clone()).list_by_tag(tag).await?,
        None => {
            let offset = (q.page.max(1) - 1) * q.limit.max(1);
            HostRepo::new(db.clone())
                .list_paged(q.limit.max(1), offset)
                .await?
        }
    };
    let agent_repo = AgentRepo::new(db);

    let mut views = Vec::with_capacity(hosts.len());
    for host in hosts {
        let agent_ids = agent_repo.list_agent_ids(host.id).await?;
        let online = registry.any_online(&agent_ids).await;
        let last_seen = agent_repo.last_heartbeat(host.id).await?;
        let stale = is_stale(last_seen, chrono::Utc::now(), timeout);
        let agent = agent_repo.first_by_host(host.id).await?;
        views.push(HostView {
            host,
            online,
            last_seen,
            stale,
            agent_id: agent.as_ref().map(|a| a.id.clone()),
            agent_version: agent.as_ref().map(|a| a.version.clone()),
            agent_elevated: agent.as_ref().map(|a| a.elevated),
        });
    }

    Ok(Json(json!({ "hosts": views })))
}

/// 创建主机：POST /api/v1/hosts
pub async fn create_host(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<CreateHostBody>,
) -> Result<Json<Value>, Error> {
    let conn_mode = if body.conn_mode == "forward" {
        "forward".to_string()
    } else {
        "reverse".to_string()
    };
    let host = HostRepo::new(state.db.clone())
        .insert(&NewHost {
            hostname: body.hostname.clone(),
            // forward 模式下目标机细节未知，拨号成功后由 Agent 回报补全
            os: String::new(),
            arch: String::new(),
            platform: String::new(),
            tags: body.tags.clone(),
            conn_mode,
            addr: body.addr,
        })
        .await?;
    let _ = AuditService::new(state.db)
        .record(
            &claims.sub,
            "host_create",
            &host.id.to_string(),
            json!({ "hostname": body.hostname }),
        )
        .await;
    Ok(Json(json!({ "host": host })))
}

/// 设置主机标签：POST /api/v1/hosts/{id}/tags
pub async fn set_host_tags(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SetTagsBody>,
) -> Result<Json<Value>, Error> {
    let host = HostRepo::new(state.db)
        .set_tags(id, &body.tags)
        .await?
        .ok_or_else(|| Error::NotFound(format!("host: {id}")))?;
    Ok(Json(json!({ "host": host })))
}

/// 更新主机：PUT /api/v1/hosts/{id}
pub async fn update_host(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateHostBody>,
) -> Result<Json<Value>, Error> {
    let conn_mode = if body.conn_mode == "forward" {
        "forward".to_string()
    } else {
        "reverse".to_string()
    };
    let host = HostRepo::new(state.db.clone())
        .update(
            id,
            &NewHost {
                hostname: body.hostname,
                os: body.os,
                arch: body.arch,
                platform: body.platform,
                tags: body.tags,
                conn_mode,
                addr: body.addr,
            },
        )
        .await?
        .ok_or_else(|| Error::NotFound(format!("host: {id}")))?;
    let _ = AuditService::new(state.db)
        .record(&claims.sub, "host_update", &id.to_string(), json!({}))
        .await;
    Ok(Json(json!({ "host": host })))
}

/// 删除主机：DELETE /api/v1/hosts/{id}（软删除）
pub async fn delete_host(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let n = HostRepo::new(state.db.clone()).soft_delete(id).await?;
    if n == 0 {
        return Err(Error::NotFound(format!("host: {id}")));
    }
    let _ = AuditService::new(state.db)
        .record(&claims.sub, "host_delete", &id.to_string(), json!({}))
        .await;
    Ok(Json(json!({ "ok": true })))
}
