//! Agent 现场编译生成端点：创建编译任务 / 查询进度 / 下载产物。

use axum::Json;
use axum::body::Body;
use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::http::header::CONTENT_DISPOSITION;
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

use crate::application::agent_generator::{AgentGenService, triple_for};
use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::listener_repo::ListenerRepo;
use serde_json::{Value, json};

/// 创建生成任务参数。
#[derive(Debug, Deserialize)]
pub struct CreateAgentGenBody {
    /// 目标操作系统：windows | linux | macos
    pub os: String,
    /// 目标架构：x86_64 | aarch64
    pub arch: String,
    /// 监听器（决定烙入的连入地址与注册 token）
    pub listener_id: Uuid,
    /// 覆盖连入地址（留空则由监听器地址推导，通配主机替换为服务器内网 IP）
    #[serde(default)]
    pub server_addr: String,
    /// 连接模式：reverse（默认）| forward
    #[serde(default)]
    pub conn_mode: String,
    /// forward 模式监听地址（默认 0.0.0.0:50052）
    #[serde(default)]
    pub listen_addr: String,
}

/// 创建生成任务：POST /api/v1/agent-gen
pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<CreateAgentGenBody>,
) -> Result<Json<Value>, Error> {
    if triple_for(&body.os, &body.arch).is_none() {
        return Err(Error::InvalidArgument(format!(
            "不支持的目标平台: {} {}",
            body.os, body.arch
        )));
    }

    let listener = ListenerRepo::new(state.db.clone())
        .get(body.listener_id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("listener: {}", body.listener_id)))?;

    let server_addr = if body.server_addr.trim().is_empty() {
        crate::application::agent_generator::resolve_server_addr(&listener.addr)
    } else {
        normalize_addr(body.server_addr.trim())
    };
    // 监听器未配置专属 auth 时回退全局 token（与监听器接入逻辑一致）
    let token = if listener.auth.is_empty() {
        state.server_token.clone()
    } else {
        listener.auth.clone()
    };

    let conn_mode = if body.conn_mode == "forward" {
        "forward"
    } else {
        "reverse"
    };
    let listen_addr = if body.listen_addr.trim().is_empty() {
        "0.0.0.0:50052".to_string()
    } else {
        body.listen_addr.trim().to_string()
    };
    let job = state.agent_gen.create(
        &body.os,
        &body.arch,
        conn_mode,
        server_addr,
        listen_addr,
        token,
    );
    let _ = AuditService::new(state.db)
        .record(
            &claims.sub,
            "agent_generate",
            &job.id.to_string(),
            json!({
                "os": body.os,
                "arch": body.arch,
                "listener": listener.name,
                "server_addr": job.server_addr,
            }),
        )
        .await;
    Ok(Json(AgentGenService::view(&job, 0)))
}

/// 列出生成任务：GET /api/v1/agent-gen
pub async fn list_jobs(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let jobs: Vec<Value> = state
        .agent_gen
        .list()
        .iter()
        .map(|j| AgentGenService::view(j, 0))
        .collect();
    Ok(Json(json!({ "jobs": jobs })))
}

/// 查询生成任务（含日志尾部）：GET /api/v1/agent-gen/{id}
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let job = state
        .agent_gen
        .get(id)
        .ok_or_else(|| Error::NotFound(format!("agent-gen job: {id}")))?;
    Ok(Json(AgentGenService::view(&job, 40)))
}

/// 下载编译产物：GET /api/v1/agent-gen/{id}/download
pub async fn download(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<axum::response::Response, Error> {
    let (path, filename) = state
        .agent_gen
        .artifact(id)
        .ok_or_else(|| Error::NotFound(format!("artifact: {id}（任务不存在或尚未就绪）")))?;

    // agent 二进制十几 MB 量级，整读入内存足够
    let bytes = tokio::fs::read(&path).await?;
    let body = Body::from(bytes);
    Ok((
        StatusCode::OK,
        [(
            CONTENT_DISPOSITION,
            format!("attachment; filename=\"{filename}\""),
        )],
        body,
    )
        .into_response())
}

/// 补全地址 scheme：裸 host:port 视为 http。
fn normalize_addr(addr: &str) -> String {
    if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("http://{addr}")
    }
}
