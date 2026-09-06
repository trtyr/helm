//! 实时流 WebSocket 端点：服务日志 tail-f / job 输出流 / 指标流。

use crate::domain::Error;
use crate::http::AppState;
use crate::http::auth::verify_bearer_token;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub token: String,
}

/// 校验 WS token（握手无法带 Authorization header，走 query param；JWT 或 API key 均可）。
async fn verify_token(state: &AppState, token: &str) -> Result<(), Error> {
    verify_bearer_token(state, token).await.map(|_| ())
}

/// 桥接：订阅 key 的增量 → 推送 WS；WS 关闭即退出。
async fn handle_stream(mut socket: WebSocket, state: AppState, key: String) {
    let mut rx = state.streams.subscribe(&key).await;
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
    verify_token(&state, &q.token).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, format!("service:{id}"))))
}

/// job 输出实时流：GET /api/v1/jobs/{id}/stream?token=
pub async fn job_stream(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, format!("job:{id}"))))
}

/// 指标实时流：GET /api/v1/metrics/stream?token=
pub async fn metrics_stream(
    State(state): State<AppState>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, "metrics".to_string())))
}

/// 通知实时流：GET /api/v1/notifications/stream?token=（决策 009：系统内小卡片推送）
pub async fn notifications_stream(
    State(state): State<AppState>,
    Query(q): Query<StreamQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_token(&state, &q.token).await?;
    Ok(ws.on_upgrade(move |socket| handle_stream(socket, state, "notifications".to_string())))
}
