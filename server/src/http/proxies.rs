//! SOCKS 代理管理端点：为 agent 开启/停止/列出 SOCKS5 监听。

use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};
use uuid::Uuid;

/// 为 agent 开启 SOCKS5 代理：POST /api/v1/proxies
pub async fn create_proxy(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, Error> {
    let agent_id = body
        .get("agent_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::InvalidArgument("agent_id is required".into()))?
        .to_string();
    let listen_addr = body
        .get("listen_addr")
        .and_then(|v| v.as_str())
        .unwrap_or("127.0.0.1:1080")
        .to_string();

    let (id, actual) = state
        .proxy_service
        .start(&agent_id, &listen_addr, state.conn_registry.clone())
        .await?;
    Ok(Json(json!({
        "id": id,
        "agent_id": agent_id,
        "listen_addr": actual,
        "status": "running",
    })))
}

/// 列出活跃代理：GET /api/v1/proxies
pub async fn list_proxies(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let proxies: Vec<Value> = state
        .proxy_service
        .list()
        .await
        .into_iter()
        .map(|(id, agent_id, addr)| {
            json!({ "id": id, "agent_id": agent_id, "listen_addr": addr, "status": "running" })
        })
        .collect();
    Ok(Json(json!({ "proxies": proxies })))
}

/// 停止代理：DELETE /api/v1/proxies/{id}
pub async fn stop_proxy(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    state.proxy_service.stop(id).await?;
    Ok(Json(json!({ "ok": true })))
}
