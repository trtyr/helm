//! 实时流 WebSocket 端点：服务日志 tail-f / job 输出流 / 指标流。

use crate::application::scopes;
use crate::domain::Error;
use crate::http::AppState;
use crate::http::auth::verify_scoped_token;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub token: String,
}

/// 校验 WS token（握手无法带 Authorization header，走 query param；JWT 或 API key 均可，
/// API key 需持有对应 scope）。
async fn verify_token(state: &AppState, token: &str, scope: &'static str) -> Result<(), Error> {
    verify_scoped_token(state, token, scope).await.map(|_| ())
}

/// 桥接：订阅 key 的增量 → 推送 WS；WS 关闭即退出。
async fn handle_stream(socket: WebSocket, state: AppState, key: String) {
    let mut rx = state.streams.subscribe(&key).await;
    pump_stream(socket, &mut rx).await;
}

/// 订阅后的推送循环（handle_stream 主体，供带快照前导的流复用）。
async fn pump_stream(socket: WebSocket, rx: &mut tokio::sync::mpsc::UnboundedReceiver<Vec<u8>>) {
    let mut socket = socket;
    loop {
        tokio::select! {
            data = rx.recv() => {
                match data {
                    Some(bytes) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}

/// 服务日志实时流：GET /api/v1/services/{id}/logs/stream?token=
pub async fn service_logs_stream(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token, scopes::SERVICES).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, format!("service:{id}"))))
}

/// 服务状态实时流（D4）：GET /api/v1/services/stream?token=
///
/// 连接建立即推送一次全量快照（`{"snapshot":[...]}`），此后 agent 上报的
/// 状态变更以增量推送（`{"service_id","status","pid","exit_code"}`）。
/// 快照与增量之间存在重复通知的可能（同服务同状态），客户端以最新为准。
pub async fn services_stream(
    State(state): State<AppState>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token, scopes::SERVICES).await?;
    Ok(ws.on_upgrade(move |socket| handle_services_stream(socket, state)))
}

/// 服务状态流主体：先订阅（防丢增量窗口）→ 推全量快照 → 进增量循环。
async fn handle_services_stream(socket: WebSocket, state: AppState) {
    let mut rx = state.streams.subscribe("services").await;
    // 全量快照：连接即给出当前全部常驻服务状态
    let services = crate::store::service_repo::ServiceRepo::new(state.db.clone())
        .list()
        .await
        .unwrap_or_default();
    let snapshot = serde_json::json!({ "snapshot": services }).to_string();
    let mut socket = socket;
    if socket
        .send(Message::Binary(snapshot.into_bytes().into()))
        .await
        .is_err()
    {
        return;
    }
    pump_stream(socket, &mut rx).await;
}

/// job 输出实时流：GET /api/v1/jobs/{id}/stream?token=
pub async fn job_stream(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token, scopes::EXEC).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, format!("job:{id}"))))
}

/// 指标实时流：GET /api/v1/metrics/stream?token=
pub async fn metrics_stream(
    State(state): State<AppState>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token, scopes::METRICS).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, "metrics".to_string())))
}

/// 内存扫描实时流：GET /api/v1/ir/memscan/{id}/stream?token=
pub async fn memscan_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token, scopes::IR).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, format!("memscan:{id}"))))
}

/// 通知实时流：GET /api/v1/notifications/stream?token=（决策 009：系统内小卡片推送）
pub async fn notifications_stream(
    State(state): State<AppState>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token, scopes::NOTIFICATIONS).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, "notifications".to_string())))
}
