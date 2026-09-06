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
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

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
    let channel = build_channel(config).await?;
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
    let proxies = crate::proxy::ProxyManager::new();
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
            Some(server_message::Kind::ProxyConnect(req)) => {
                proxies.connect(&req.conn_id, &req.target, &tx).await;
            }
            Some(server_message::Kind::ProxyData(req)) => {
                proxies.data(&req.conn_id, &req.data).await;
            }
            Some(server_message::Kind::ProxyClose(req)) => {
                proxies.close(&req.conn_id).await;
            }
            Some(server_message::Kind::SysServiceList(req)) => {
                let msg = crate::sys_service::list_services(&req.request_id);
                let _ = tx.send(msg).await;
            }
            Some(server_message::Kind::SysServiceAction(req)) => {
                let msg =
                    crate::sys_service::service_action(&req.request_id, &req.name, &req.action);
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
pub(crate) fn build_register(config: &Config) -> Register {
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
            local_ips: collect_local_ips(),
        }),
        version: env!("CARGO_PKG_VERSION").to_string(),
    }
}

/// 采集本机内网 IP：非回环地址，IPv4 在前（列表展示取首个 v4）。
fn collect_local_ips() -> Vec<String> {
    let mut ips: Vec<String> = sysinfo::Networks::new_with_refreshed_list()
        .values()
        .flat_map(|data| data.ip_networks().iter().map(|ip| ip.addr.to_string()))
        .filter(|s| {
            !s.parse::<std::net::IpAddr>()
                .map_or(true, |a| a.is_loopback())
        })
        .collect();
    ips.sort_by_key(|s| s.contains(':')); // IPv4 在前
    ips.dedup();
    ips
}

pub(crate) fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// 建立到 Server 的 channel：cert_dir 非空时走 mTLS（换证书 + 双向认证）。
async fn build_channel(config: &Config) -> Result<Channel> {
    if config.cert_dir.is_empty() {
        return Ok(Channel::from_shared(config.server_addr.clone())?
            .connect()
            .await?);
    }

    let http_addr = if config.server_http_addr.is_empty() {
        derive_http_addr(&config.server_addr)
    } else {
        config.server_http_addr.clone()
    };

    let cert = crate::cert::obtain(
        &config.cert_dir,
        &config.agent_id,
        &config.token,
        &http_addr,
    )
    .await?;

    let client_tls = ClientTlsConfig::new()
        .domain_name(config.tls_server_name.clone())
        .ca_certificate(Certificate::from_pem(cert.ca_pem))
        .identity(Identity::from_pem(cert.cert_pem, cert.key_pem));

    // tonic 仅在 scheme 为 https 时才走 TLS（见 connector.rs 的 is_https 判断）
    let tls_addr = to_https_addr(&config.server_addr);
    Ok(Channel::from_shared(tls_addr)?
        .tls_config(client_tls)?
        .connect()
        .await?)
}

/// 把 http:// 转 https://（mTLS 连接用），其余原样。
fn to_https_addr(server_addr: &str) -> String {
    if let Some(rest) = server_addr.strip_prefix("http://") {
        format!("https://{rest}")
    } else {
        server_addr.to_string()
    }
}

/// 由 gRPC 地址推导 HTTP 地址（换证书用），默认端口 18080。
fn derive_http_addr(server_addr: &str) -> String {
    match server_addr.find("://") {
        Some(i) => {
            let rest = &server_addr[i + 3..];
            match rest.rsplit_once(':') {
                Some((host, _)) => format!("{}://{}:18080", &server_addr[..i], host),
                None => server_addr.to_string(),
            }
        }
        None => server_addr.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_http_addr_replaces_port() {
        assert_eq!(
            derive_http_addr("http://127.0.0.1:50051"),
            "http://127.0.0.1:18080"
        );
        assert_eq!(
            derive_http_addr("https://example.com:8443"),
            "https://example.com:18080"
        );
        assert_eq!(derive_http_addr("http://host"), "http://host");
    }

    #[test]
    fn to_https_addr_swaps_scheme() {
        assert_eq!(
            to_https_addr("http://127.0.0.1:50051"),
            "https://127.0.0.1:50051"
        );
        assert_eq!(to_https_addr("https://h:50051"), "https://h:50051");
        assert_eq!(to_https_addr("h:50051"), "h:50051");
    }
}
