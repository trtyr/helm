//! 交互终端 WebSocket 端点：GET /api/v1/agents/{id}/terminal?token=...

use crate::domain::Error;
use crate::http::AppState;
use crate::http::auth::verify_bearer_token;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use helm_proto::pb::{
    ServerMessage, SessionClose, SessionInput, SessionOpen, SessionResize, server_message,
};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct TerminalQuery {
    pub token: String,
    /// 初始列数（默认 80）。
    pub cols: Option<u32>,
    /// 初始行数（默认 24）。
    pub rows: Option<u32>,
}

/// 二进制帧首字节标记（浏览器 → Server）。
const FRAME_INPUT: u8 = 0x01;
const FRAME_RESIZE: u8 = 0x02;

/// WebSocket 终端端点（认证走 query param，因 WS 握手无法带 Authorization header；JWT 或 API key 均可）。
pub async fn terminal(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    verify_bearer_token(&state, &query.token).await?;

    if !state.registry.is_online(&agent_id).await {
        return Err(Error::NotConnected(agent_id));
    }

    Ok(ws.on_upgrade(move |socket| {
        handle_socket(
            socket,
            state,
            agent_id,
            query.cols.unwrap_or(80),
            query.rows.unwrap_or(24),
        )
    }))
}

/// WS 桥接：输入 → Agent，输出 → WS。
///
/// 浏览器 → Server 二进制帧协议：首字节 `0x01` = PTY 输入（其余字节直通）；
/// `0x02` = resize（后续 UTF-8 JSON `{"cols":u16,"rows":u16}` → SessionResize）。
/// Text 帧按输入直通（向后兼容）。
async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    agent_id: String,
    cols: u32,
    rows: u32,
) {
    let session_id = Uuid::new_v4().to_string();

    let open = ServerMessage {
        kind: Some(server_message::Kind::SessionOpen(SessionOpen {
            session_id: session_id.clone(),
            cols,
            rows,
            command: String::new(),
        })),
    };
    if state.registry.send(&agent_id, open).await.is_err() {
        let _ = socket.send(Message::Close(None)).await;
        return;
    }

    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    state.sessions.register(&session_id, out_tx).await;

    let idle = std::time::Duration::from_secs(state.session_idle_timeout_secs);

    loop {
        tokio::select! {
            msg = socket.recv() => {
                let data: Option<Vec<u8>> = match msg {
                    Some(Ok(Message::Text(t))) => Some(t.as_bytes().to_vec()),
                    Some(Ok(Message::Binary(b))) => Some(b.to_vec()),
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => None,
                };
                let Some(data) = data else { continue };
                // 首字节分流：输入 / resize
                let msg = match data.split_first() {
                    Some((&FRAME_RESIZE, rest)) => {
                        match serde_json::from_slice::<(u32, u32)>(rest) {
                            Ok((c, r)) => ServerMessage {
                                kind: Some(server_message::Kind::SessionResize(SessionResize {
                                    session_id: session_id.clone(),
                                    cols: c,
                                    rows: r,
                                })),
                            },
                            Err(_) => continue, // 非法 resize 帧丢弃
                        }
                    }
                    Some((&FRAME_INPUT, rest)) => ServerMessage {
                        kind: Some(server_message::Kind::SessionInput(SessionInput {
                            session_id: session_id.clone(),
                            data: rest.to_vec(),
                        })),
                    },
                    // 无标记（Text 帧或老客户端）：整帧按输入
                    _ => ServerMessage {
                        kind: Some(server_message::Kind::SessionInput(SessionInput {
                            session_id: session_id.clone(),
                            data,
                        })),
                    },
                };
                if state.registry.send(&agent_id, msg).await.is_err() {
                    break;
                }
            }
            data = out_rx.recv() => {
                match data {
                    Some(bytes) => {
                        if socket.send(Message::Binary(bytes.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(idle) => {
                tracing::info!(session_id = %session_id, "session idle timeout, closing");
                // 告知前端关闭原因（前端区分空闲超时与异常断开）
                let _ = socket
                    .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: axum::extract::ws::close_code::NORMAL,
                        reason: "idle_timeout".into(),
                    })))
                    .await;
                break;
            }
        }
    }

    let close = ServerMessage {
        kind: Some(server_message::Kind::SessionClose(SessionClose {
            session_id: session_id.clone(),
        })),
    };
    let _ = state.registry.send(&agent_id, close).await;
    state.sessions.unregister(&session_id).await;
}
