//! 交互终端 WebSocket 端点：GET /api/v1/agents/{id}/terminal?token=...

use crate::application::auth_service::AuthService;
use crate::domain::Error;
use crate::http::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::response::IntoResponse;
use helm_proto::pb::{ServerMessage, SessionClose, SessionInput, SessionOpen, server_message};
use serde::Deserialize;
use tokio::sync::mpsc;
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct TerminalQuery {
    pub token: String,
}

/// WebSocket 终端端点（认证走 query param，因 WS 握手无法带 Authorization header）。
pub async fn terminal(
    State(state): State<AppState>,
    Path(agent_id): Path<String>,
    Query(query): Query<TerminalQuery>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, Error> {
    let auth = AuthService::new(state.db.clone(), state.jwt_secret.clone());
    auth.verify(&query.token)
        .map_err(|_| Error::Unauthorized("invalid token".into()))?;

    if !state.registry.is_online(&agent_id).await {
        return Err(Error::NotConnected(agent_id));
    }

    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, agent_id)))
}

/// WS 桥接：输入 → Agent，输出 → WS。
async fn handle_socket(mut socket: WebSocket, state: AppState, agent_id: String) {
    let session_id = Uuid::new_v4().to_string();

    let open = ServerMessage {
        kind: Some(server_message::Kind::SessionOpen(SessionOpen {
            session_id: session_id.clone(),
            cols: 80,
            rows: 24,
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
                if let Some(data) = data {
                    let input = ServerMessage {
                        kind: Some(server_message::Kind::SessionInput(SessionInput {
                            session_id: session_id.clone(),
                            data,
                        })),
                    };
                    if state.registry.send(&agent_id, input).await.is_err() {
                        break;
                    }
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
