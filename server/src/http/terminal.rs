//! 交互终端 WebSocket 端点：GET /api/v1/agents/{id}/terminal?token=...

use crate::application::scopes;
use crate::domain::Error;
use crate::http::AppState;
use crate::http::auth::verify_scoped_token;
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
    verify_scoped_token(&state, &query.token, scopes::EXEC).await?;

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
///
/// **G11 拆分（2026-09-21）**：原为 102 行单块。现拆为——`pump_terminal`（三路 select 主循环）/
/// `client_frame_to_msg`（客户端帧 → 上行消息）/ `teardown_session`（回收）；本函数只做
/// 开会话、注册通道、进入泵、收尾这条编排。
async fn handle_socket(
    mut socket: WebSocket,
    state: AppState,
    agent_id: String,
    cols: u32,
    rows: u32,
) {
    let session_id = Uuid::new_v4().to_string();

    // 阶段 1：向 agent 开一个 PTY 会话；失败即关掉前端连接
    let open = ServerMessage {
        kind: Some(server_message::Kind::SessionOpen(SessionOpen {
            session_id: session_id.clone(),
            cols,
            rows,
            command: String::new(),
        })),
    };
    if state.registry.send(&agent_id, open).await.is_err() {
        let _ = socket.send(Message::Close(None)).await; // 前端 socket 已断：告知失败也无处送达，直接退出
        return;
    }

    // 阶段 2：注册输出通道并进入双向泵
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    state.sessions.register(&session_id, out_tx).await;
    let idle = std::time::Duration::from_secs(state.session_idle_timeout_secs);
    pump_terminal(
        &mut socket,
        &state,
        &agent_id,
        &session_id,
        &mut out_rx,
        idle,
    )
    .await;

    // 阶段 3：回收（通知 agent 关闭会话 + 注销输出通道）
    teardown_session(&state, &agent_id, &session_id).await;
}

/// 阶段 2：三路 select——客户端帧上行 / agent 输出下行 / 空闲超时（任一触发即退出）。
async fn pump_terminal(
    socket: &mut WebSocket,
    state: &AppState,
    agent_id: &str,
    session_id: &str,
    out_rx: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    idle: std::time::Duration,
) {
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
                let Some(msg) = client_frame_to_msg(data, session_id) else { continue };
                if state.registry.send(agent_id, msg).await.is_err() {
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
                // 前端 socket 若已断，关闭帧无处送达——本次会话本来就结束，无需上报
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
}

/// 客户端帧 → 上行消息：首字节分流（`0x02` resize / `0x01` 输入 / 无标记整帧按输入）。
/// 非法 resize 帧返回 `None`（丢弃该帧）。
fn client_frame_to_msg(data: Vec<u8>, session_id: &str) -> Option<ServerMessage> {
    match data.split_first() {
        Some((&FRAME_RESIZE, rest)) => match serde_json::from_slice::<(u32, u32)>(rest) {
            Ok((c, r)) => Some(ServerMessage {
                kind: Some(server_message::Kind::SessionResize(SessionResize {
                    session_id: session_id.to_string(),
                    cols: c,
                    rows: r,
                })),
            }),
            Err(_) => None, // 非法 resize 帧丢弃
        },
        Some((&FRAME_INPUT, rest)) => Some(ServerMessage {
            kind: Some(server_message::Kind::SessionInput(SessionInput {
                session_id: session_id.to_string(),
                data: rest.to_vec(),
            })),
        }),
        // 无标记（Text 帧或老客户端）：整帧按输入
        _ => Some(ServerMessage {
            kind: Some(server_message::Kind::SessionInput(SessionInput {
                session_id: session_id.to_string(),
                data,
            })),
        }),
    }
}

/// 阶段 3：会话回收——通知 agent 关闭 session，并注销输出通道。
async fn teardown_session(state: &AppState, agent_id: &str, session_id: &str) {
    let close = ServerMessage {
        kind: Some(server_message::Kind::SessionClose(SessionClose {
            session_id: session_id.to_string(),
        })),
    };
    let _ = state.registry.send(agent_id, close).await; // agent 已断：关闭指令无需送达，会话本地照常回收
    state.sessions.unregister(session_id).await;
}
