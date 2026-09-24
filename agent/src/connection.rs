//! 与 Server 的连接：建立双向流、注册、心跳、断线重连。

use std::time::Duration;

use crate::config::Config;
use anyhow::Result;
use helm_proto::pb::{
    AgentMessage, Heartbeat, HostInfo, Register, agent_message,
    agent_service_client::AgentServiceClient,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Identity};

mod session;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// 重连退避基数（B7）：普通网络错误从 3s 起指数退避。
const RECONNECT_BASE: Duration = Duration::from_secs(3);
/// 重连退避上限。
const RECONNECT_MAX: Duration = Duration::from_secs(300);
/// unauthenticated（token 配错等不可自愈错误）的长退避：显著放缓自伤式重试，
/// 并以 error 级日志保证本地可见（B7）。
const RECONNECT_AUTH_FAILURE: Duration = Duration::from_secs(300);

/// 下一次重连延迟（纯函数便于测试；`jitter` 为 0..=400 的伪随机量）。
///
/// - `auth_failure`：token 配错类不可自愈错误 → 固定长退避（300s），不再高频冲击 Server；
/// - 普通错误：指数退避 `min(base * 2^(attempt-1), max)`，附加 ±20% jitter 防止集群同步重试。
fn next_reconnect_delay(attempt: u32, auth_failure: bool, jitter: u32) -> Duration {
    if auth_failure {
        return RECONNECT_AUTH_FAILURE;
    }
    let exp = attempt.saturating_sub(1).min(16);
    let base = RECONNECT_BASE
        .saturating_mul(1u32 << exp)
        .min(RECONNECT_MAX);
    // jitter ∈ [0,400] 线性映射偏移 [-20%, +20%]
    let offset_ms = base.as_millis() as i64 * (jitter.min(400) as i64 - 200) / 1000;
    Duration::from_millis((base.as_millis() as i64 + offset_ms).max(0) as u64)
}

/// 错误是否为认证失败（token 配错等，不可自愈）。
fn is_auth_failure(e: &anyhow::Error) -> bool {
    e.downcast_ref::<tonic::Status>()
        .map(|s| s.code() == tonic::Code::Unauthenticated)
        .unwrap_or(false)
}

/// 连接循环：断线后自动重连并重新注册（B7：指数退避 + jitter + 认证错误长退避）。
pub async fn run_agent(config: &Config) -> Result<()> {
    let mut attempt: u32 = 0;
    loop {
        let result = connect_once(config).await;
        let auth_failure = matches!(&result, Err(e) if is_auth_failure(e));
        match &result {
            Ok(()) => {
                attempt = 0;
                tracing::warn!("connection closed, reconnecting");
            }
            Err(e) => {
                attempt += 1;
                if auth_failure {
                    // 不可自愈：error 级本地日志（运维可见）+ 长退避
                    tracing::error!(
                        attempt,
                        delay = ?RECONNECT_AUTH_FAILURE,
                        error = %e,
                        "authentication failed (check HELM_AGENT_TOKEN); retrying with long backoff"
                    );
                } else {
                    tracing::warn!(attempt, error = %e, "connection failed, reconnecting");
                }
            }
        }
        let jitter = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|t| t.subsec_nanos() % 401)
            .unwrap_or(0);
        let delay = next_reconnect_delay(attempt, auth_failure, jitter);
        tracing::debug!(?delay, attempt, "reconnect scheduled");
        tokio::time::sleep(delay).await;
    }
}

/// 单次连接：建立双向流 → 发送 Register → 心跳 → 直到断开。
async fn connect_once(config: &Config) -> Result<()> {
    let channel = build_channel(config).await?;
    let mut client = AgentServiceClient::new(channel);

    // outbound：先发 Register，之后由心跳 task 持续发 Heartbeat。
    let (tx, rx) = mpsc::channel::<AgentMessage>(64);
    send_register(&tx, config).await?;
    let _heartbeat = spawn_heartbeat(tx.clone());
    let _monitor = spawn_monitor(tx.clone());

    // 建立双向流
    let response = client.open_channel(ReceiverStream::new(rx)).await?;
    let mut inbound = response.into_inner();
    tracing::info!(agent_id = %config.agent_id, "channel opened, waiting for register ack");

    // 会话生命周期：逐条消费下行消息，直到流结束（或注册被拒 → 终止本次连接）
    let mut session = session::AgentSession::new(config.agent_id.clone(), tx);
    while let Some(msg) = inbound.message().await? {
        session.handle(msg.kind).await?;
    }
    Ok(())
}

/// 首帧：Register（协议与 forward 同构，Server 据此注册）。
async fn send_register(tx: &mpsc::Sender<AgentMessage>, config: &Config) -> Result<()> {
    tx.send(AgentMessage {
        kind: Some(agent_message::Kind::Register(build_register(config))),
    })
    .await?;
    Ok(())
}

/// 心跳 task：连接断开（tx 关闭）即自行退出。
fn spawn_heartbeat(tx: mpsc::Sender<AgentMessage>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            let msg = AgentMessage {
                kind: Some(agent_message::Kind::Heartbeat(Heartbeat {
                    timestamp_unix_ms: now_ms(),
                })),
            };
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    })
}

/// 监控 task（指标上报）。
fn spawn_monitor(tx: mpsc::Sender<AgentMessage>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        crate::monitor::run_monitor(tx).await;
    })
}

/// 构造 Register 消息，附带目标主机信息。
pub(crate) fn build_register(config: &Config) -> Register {
    let hostname = hostname::get()
        .ok()
        .and_then(|h| h.into_string().ok())
        .unwrap_or_else(|| "unknown".to_string());
    let os = std::env::consts::OS.to_string();
    let arch = std::env::consts::ARCH.to_string();

    // 版本细节（sysinfo 跨平台：Windows 出 10 (19045)，Linux 出 24.04.3 / 内核号）
    let os_version = sysinfo::System::long_os_version().unwrap_or_default();
    let kernel = sysinfo::System::kernel_version().unwrap_or_default();
    let uptime_secs = sysinfo::System::uptime();

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
            elevated: crate::privilege::is_elevated(),
            os_version,
            kernel,
            uptime_secs,
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
        // EN-72：h2 层 PING 保活——半开连接 40s 内快速检测并干净重连（B7 接管），
        // 不再依赖对端 RST/应用层心跳；keep_alive_while_idle 覆盖空闲窗口
        return Ok(Channel::from_shared(config.server_addr.clone())?
            .http2_keep_alive_interval(Duration::from_secs(30))
            .keep_alive_timeout(Duration::from_secs(10))
            .keep_alive_while_idle(true)
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
    // B8：mTLS 分支与明文分支同享 h2 保活——半开连接 40s 内快速检测（EN-72 补全）
    Ok(Channel::from_shared(tls_addr)?
        .tls_config(client_tls)?
        .http2_keep_alive_interval(Duration::from_secs(30))
        .keep_alive_timeout(Duration::from_secs(10))
        .keep_alive_while_idle(true)
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

/// 由 gRPC 地址推导 HTTP 地址（换证书用），默认端口 8080（与 Server 默认 HTTP 端口一致）。
fn derive_http_addr(server_addr: &str) -> String {
    match server_addr.find("://") {
        Some(i) => {
            let rest = &server_addr[i + 3..];
            match rest.rsplit_once(':') {
                Some((host, _)) => format!("{}://{}:8080", &server_addr[..i], host),
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
    fn reconnect_backoff_is_exponential_with_cap() {
        // 普通错误：3s 起指数退避（jitter=200 即零偏移），封顶 300s
        assert_eq!(next_reconnect_delay(0, false, 200), Duration::from_secs(3));
        assert_eq!(next_reconnect_delay(1, false, 200), Duration::from_secs(3));
        assert_eq!(next_reconnect_delay(2, false, 200), Duration::from_secs(6));
        assert_eq!(next_reconnect_delay(3, false, 200), Duration::from_secs(12));
        assert_eq!(
            next_reconnect_delay(20, false, 200),
            Duration::from_secs(300),
            "must cap at RECONNECT_MAX"
        );
    }

    #[test]
    fn reconnect_backoff_applies_jitter() {
        // jitter ∈ [0,400] → ±20% 偏移：3s → [2.4s, 3.6s]
        let low = next_reconnect_delay(1, false, 0);
        let high = next_reconnect_delay(1, false, 400);
        assert_eq!(low, Duration::from_millis(2400));
        assert_eq!(high, Duration::from_millis(3600));
        // jitter=200 → 零偏移（中位）
        assert_eq!(next_reconnect_delay(1, false, 200), Duration::from_secs(3));
    }

    #[test]
    fn auth_failure_uses_long_backoff() {
        // token 配错（不可自愈）：固定 300s，与 attempt 无关
        assert_eq!(next_reconnect_delay(1, true, 0), Duration::from_secs(300));
        assert_eq!(next_reconnect_delay(9, true, 400), Duration::from_secs(300));
    }

    #[test]
    fn auth_failure_detection_from_tonic_status() {
        let err: anyhow::Error = tonic::Status::unauthenticated("bad token").into();
        assert!(is_auth_failure(&err));
        let err: anyhow::Error = tonic::Status::unavailable("down").into();
        assert!(!is_auth_failure(&err));
        let err: anyhow::Error = anyhow::anyhow!("plain io error");
        assert!(!is_auth_failure(&err));
    }

    #[test]
    fn derive_http_addr_replaces_port() {
        assert_eq!(
            derive_http_addr("http://127.0.0.1:50051"),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            derive_http_addr("https://example.com:8443"),
            "https://example.com:8080"
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
