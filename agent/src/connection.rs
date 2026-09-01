//! 与 Server 的连接：建立双向流、注册、心跳、断线重连。

use std::time::Duration;

use crate::config::Config;
use anyhow::{Result, anyhow};
use helm_proto::pb::{
    AgentMessage, Heartbeat, HostInfo, Register, SessionOpened, agent_message,
    agent_service_client::AgentServiceClient, server_message,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const RECONNECT_DELAY: Duration = Duration::from_secs(3);

/// 连接循环：断线后自动重连并重新注册。
pub async fn run_agent(config: &Config) -> Result<()> {
    loop {
        match connect_once(config).await {
            Ok(()) => {
                tracing::warn!("connection closed, reconnecting in {RECONNECT_DELAY:?}");
            }
            Err(e) => {
                tracing::warn!(error = %e, "connection failed, reconnecting in {RECONNECT_DELAY:?}");
            }
        }
        tokio::time::sleep(RECONNECT_DELAY).await;
    }
}

/// 单次连接：建立双向流 → 发送 Register → 心跳 → 直到断开。
async fn connect_once(config: &Config) -> Result<()> {
    let channel = Channel::from_shared(config.server_addr.clone())?
        .connect()
        .await?;
    let mut client = AgentServiceClient::new(channel);

    // outbound：先发 Register，之后由心跳 task 持续发 Heartbeat。
    let (tx, rx) = mpsc::channel::<AgentMessage>(64);
    tx.send(AgentMessage {
        kind: Some(agent_message::Kind::Register(build_register(config))),
    })
    .await?;

    // 心跳 task
    let tx_hb = tx.clone();
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            let msg = AgentMessage {
                kind: Some(agent_message::Kind::Heartbeat(Heartbeat {
                    timestamp_unix_ms: now_ms(),
                })),
            };
            if tx_hb.send(msg).await.is_err() {
                break;
            }
        }
    });

    // 监控 task
    let tx_mon = tx.clone();
    tokio::spawn(async move {
        crate::monitor::run_monitor(tx_mon).await;
    });

    // 建立双向流
    let response = client.open_channel(ReceiverStream::new(rx)).await?;
    let mut inbound = response.into_inner();
    let mut file_handler = crate::file::FileHandler::new();
    let sessions = crate::pty::SessionManager::new();
    let services = crate::service::ServiceManager::new();

    tracing::info!(agent_id = %config.agent_id, "channel opened, waiting for register ack");

    while let Some(msg) = inbound.message().await? {
        match msg.kind {
            Some(server_message::Kind::RegisterAck(ack)) => {
                if ack.ok {
                    tracing::info!(
                        agent_id = %config.agent_id,
                        message = %ack.message,
                        "registered"
                    );
                } else {
                    return Err(anyhow!("register rejected: {}", ack.message));
                }
            }
            Some(server_message::Kind::ExecRequest(req)) => {
                let tx = tx.clone();
                tokio::spawn(async move {
                    crate::exec::run_and_report(&req.job_id, &req.command, &req.args, &tx).await;
                });
            }
            Some(server_message::Kind::FileRequest(req)) => {
                file_handler.handle_request(req, &tx).await;
            }
            Some(server_message::Kind::FileChunk(chunk)) => {
                file_handler.handle_chunk(chunk, &tx).await;
            }
            Some(server_message::Kind::SelfDestruct(sd)) => {
                tracing::warn!(
                    agent_id = %config.agent_id,
                    remove_binary = sd.remove_binary,
                    "uninstall command received"
                );
                crate::uninstall::self_destruct(sd.remove_binary);
            }
            Some(server_message::Kind::SessionOpen(req)) => {
                let tx_out = tx.clone();
                match sessions.open(
                    &req.session_id,
                    req.cols as u16,
                    req.rows as u16,
                    &req.command,
                    tx_out.clone(),
                ) {
                    Ok(()) => {
                        let _ = tx_out
                            .send(AgentMessage {
                                kind: Some(agent_message::Kind::SessionOpened(SessionOpened {
                                    session_id: req.session_id.clone(),
                                })),
                            })
                            .await;
                    }
                    Err(e) => {
                        tracing::warn!(session_id = %req.session_id, error = %e, "session open failed");
                    }
                }
            }
            Some(server_message::Kind::SessionInput(req)) => {
                sessions.input(&req.session_id, &req.data);
            }
            Some(server_message::Kind::SessionClose(req)) => {
                sessions.close(&req.session_id);
            }
            Some(server_message::Kind::SessionResize(req)) => {
                sessions.resize(&req.session_id, req.cols as u16, req.rows as u16);
            }
            Some(server_message::Kind::ServiceStart(req)) => {
                services
                    .start(
                        &req.service_id,
                        &req.command,
                        &req.args,
                        &req.restart_policy,
                        tx.clone(),
                    )
                    .await;
            }
            Some(server_message::Kind::ServiceStop(req)) => {
                services.stop(&req.service_id).await;
            }
            Some(server_message::Kind::FileList(req)) => {
                let msg = crate::fs::list_dir(&req.request_id, &req.path);
                let _ = tx.send(msg).await;
            }
            Some(server_message::Kind::ProcessList(req)) => {
                let msg = crate::process::list_processes(&req.request_id);
                let _ = tx.send(msg).await;
            }
            Some(server_message::Kind::ProcessKill(req)) => {
                let msg = crate::process::kill_process(&req.request_id, req.pid);
                let _ = tx.send(msg).await;
            }
            Some(server_message::Kind::NetInfo(req)) => {
                let msg = crate::process::net_info(&req.request_id);
                let _ = tx.send(msg).await;
            }
            other => {
                tracing::debug!(agent_id = %config.agent_id, ?other, "server message (later phase)");
            }
        }
    }

    Ok(())
}

/// 构造 Register 消息，附带目标主机信息。
fn build_register(config: &Config) -> Register {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    Register {
        agent_id: config.agent_id.clone(),
        token: config.token.clone(),
        host: Some(HostInfo {
            hostname,
            os: os.clone(),
            arch: arch.clone(),
            platform: format!("{os}-{arch}"),
            tags: Vec::new(),
        }),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
