//! forward 模式的**一次拨号**流程（G10 阶段化 + 文件体积拆分，2026-09-21）。
//!
//! 自 `forward_manager.rs` 拆出：连接建立核心链路——建流 → Register 校验 → 落库 →
//! 登记连接 → 上线通知 → 回 RegisterAck → 交给入站泵。
//!
//! 阶段划分与 reverse 侧 `AgentServiceImpl::open_channel` **同构**：两侧不同构时，
//! 「一个模式修了、另一个忘了」这类分叉会再次发生（`forward_manager.rs` 顶部注释里
//! 记的 P003 T1 审计缺陷就是一次实例）。入站消费（`pump_forward_inbound` /
//! `forward_inbound_loop`）留在父模块——它是两种注册模式共用的下游。

use super::{ForwardDeps, pump_forward_inbound};
use crate::application::agent_lifecycle_service::AgentLifecycleService;
use crate::grpc::connection_registry::RegisteredConnection;
use helm_proto::pb::{
    AgentMessage, Register, RegisterAck, ServerMessage, agent_message,
    forward_agent_service_client::ForwardAgentServiceClient, server_message,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Streaming;
use tonic::transport::{Certificate, ClientTlsConfig, Identity};
use uuid::Uuid;

/// 拨号一次：建流 → 等 agent Register → 校验 token → 落库注册 → 消费入站流。
///
/// 本函数只做阶段编排（阶段 1-7），每个阶段的细节在各自的具名函数里。
pub(super) async fn connect_once(
    host_id: Uuid,
    addr: &str,
    deps: &ForwardDeps,
    stop_rx: &mut mpsc::Receiver<()>,
) -> anyhow::Result<()> {
    // 阶段 1：建流（含可选 mTLS 双向认证）并打开双向流
    let (tx, mut inbound) = open_forward_stream(deps, addr).await?;

    // 阶段 2：首帧 Register + token 校验（A2 可接受全集）
    let register = read_forward_register(&mut inbound, deps).await?;
    let agent_id = register.agent_id.clone();

    // 阶段 3：落库——挂到声明的 forward host 下（不新建 host）
    persist_forward_agent(deps, host_id, &agent_id, &register, addr).await?;

    // 阶段 4：登记连接；容量满（E3）时回 RegisterAck(ok=false) 并正常结束本次拨号
    let Some(registration) = register_forward_connection(deps, host_id, addr, &register, &tx).await
    else {
        return Ok(());
    };

    // 阶段 5：上线通知（决策 009：forward 与 reverse 同构；重复注册仅换连接，不通知）
    notify_forward_online(deps, host_id, &register, registration.replaced.is_some()).await;

    // 阶段 6：回 RegisterAck
    ack_forward_register(&tx).await;

    // 阶段 7：入站循环（与 reverse 同构），退出时统一记账
    pump_forward_inbound(
        deps,
        host_id,
        &agent_id,
        register,
        registration,
        inbound,
        stop_rx,
    )
    .await
}

/// 阶段 1：建流（含可选 mTLS 双向认证）并打开 forward 双向流。
///
/// 返回「发往 agent 的发送端」与「来自 agent 的入站流」——注册阶段的落库/登记在后续阶段进行。
async fn open_forward_stream(
    deps: &ForwardDeps,
    addr: &str,
) -> anyhow::Result<(mpsc::Sender<ServerMessage>, Streaming<AgentMessage>)> {
    // mTLS 启用时走 https（tonic 仅 https scheme 走 TLS），并出示证书双向认证
    let scheme = if deps.cert.enabled() { "https" } else { "http" };
    let uri = if addr.starts_with("http://") || addr.starts_with("https://") {
        addr.to_string()
    } else {
        format!("{scheme}://{addr}")
    };
    let mut endpoint = tonic::transport::Channel::from_shared(uri)
        .map_err(|e| anyhow::anyhow!("bad addr: {e}"))?;
    if deps.cert.enabled() {
        endpoint = endpoint.tls_config(forward_tls_config(deps))?;
    }
    // B8：forward 拨号同样开启 h2 保活——半开连接 40s 内快速检测并触发重连
    let endpoint = endpoint
        .http2_keep_alive_interval(std::time::Duration::from_secs(30))
        .keep_alive_timeout(std::time::Duration::from_secs(10))
        .keep_alive_while_idle(true);
    let channel = endpoint.connect().await?;
    let mut client = ForwardAgentServiceClient::new(channel);

    let (tx, rx) = mpsc::channel::<ServerMessage>(64);
    let inbound = client
        .open_forward_channel(ReceiverStream::new(rx))
        .await?
        .into_inner();
    Ok((tx, inbound))
}

/// mTLS 客户端配置：CA 校验对端 + 出示本 Server 证书做双向认证。
fn forward_tls_config(deps: &ForwardDeps) -> ClientTlsConfig {
    ClientTlsConfig::new()
        .domain_name(deps.tls_server_name.clone())
        .ca_certificate(Certificate::from_pem(deps.cert.ca_cert_pem().as_bytes()))
        .identity(Identity::from_pem(
            deps.cert.server_cert_pem().as_bytes(),
            deps.cert.server_key_pem().as_bytes(),
        ))
}

/// 阶段 2：首条消息必须是 agent 主动发的 Register，且 token 必须命中可接受全集（A2）。
///
/// 校验失败即 `Err`：调用方（`run_host_loop`）记 warn 并按 `RECONNECT_DELAY` 重试——
/// 无效 token 的 agent 会持续被拒，不会进入注册表。
async fn read_forward_register(
    inbound: &mut Streaming<AgentMessage>,
    deps: &ForwardDeps,
) -> anyhow::Result<Register> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| anyhow::anyhow!("channel closed before Register"))?;
    let register = match first.kind {
        Some(agent_message::Kind::Register(r)) => r,
        _ => anyhow::bail!("first message must be Register"),
    };
    if !crate::grpc::agent_service::token_matches_any(&deps.server_tokens, &register.token) {
        anyhow::bail!("invalid token");
    }
    Ok(register)
}

/// 阶段 3：落库——挂到声明的 forward host 下（不新建 host）。
///
/// public_ip 取拨号地址的 IP 部分（forward 场景 Server 主动拨出，该地址即对外可达地址）。
/// **G3 收口（2026-09-21）**：经 `AgentLifecycleService`，grpc 层不再直构 `AgentRepo`。
async fn persist_forward_agent(
    deps: &ForwardDeps,
    host_id: Uuid,
    agent_id: &str,
    register: &Register,
    addr: &str,
) -> anyhow::Result<()> {
    let public_ip = addr.split(':').next().unwrap_or_default().to_string();
    AgentLifecycleService::new(deps.db.clone(), deps.registry.clone())
        .register_under_host(host_id, agent_id, register, &public_ip)
        .await?;
    Ok(())
}

/// 阶段 4：登记连接。容量满（E3）即回 `RegisterAck(ok=false)` 并放弃本次拨号；
/// 同 id 重复注册则顶掉旧连接。返回 `None` = 「已拒绝，本次拨号正常结束」。
async fn register_forward_connection(
    deps: &ForwardDeps,
    host_id: Uuid,
    addr: &str,
    register: &Register,
    tx: &mpsc::Sender<ServerMessage>,
) -> Option<RegisteredConnection> {
    let agent_id = register.agent_id.clone();
    match deps.registry.register(&agent_id, tx.clone()).await {
        Ok(registration) => {
            if let Some(old) = registration.replaced.as_ref() {
                tracing::info!(agent_id = %agent_id, "duplicate registration (forward), kicking previous connection");
                old.kick();
            }
            tracing::info!(
                host_id = %host_id,
                agent_id = %agent_id,
                version = %register.version,
                addr = %addr,
                "forward agent registered"
            );
            Some(registration)
        }
        Err(e) => {
            // E3：注册表容量已满——拒绝注册（不广播上线），agent 侧会按退避重试
            tracing::error!(agent_id = %agent_id, error = %e, "registration rejected: registry full (forward)");
            let ack = ServerMessage {
                kind: Some(server_message::Kind::RegisterAck(RegisterAck {
                    ok: false,
                    message: format!("server at capacity: {e}"),
                    heartbeat_interval_secs: 10,
                })),
            };
            // 对端已断即本次拨号会话结束（此路径随即 return）
            let _ = tx.send(ack).await;
            None
        }
    }
}

/// 阶段 5：上线通知（决策 009：forward 与 reverse 同构；重复注册仅换连接，不通知）。
async fn notify_forward_online(
    deps: &ForwardDeps,
    host_id: Uuid,
    register: &Register,
    replaced: bool,
) {
    if replaced {
        return;
    }
    let hostname = register
        .host
        .as_ref()
        .map(|h| h.hostname.as_str())
        .unwrap_or("");
    // P003 T1：与 reverse 共用 record_online_and_notify（审计缺陷修复：此前 forward 只发通知、
    // 不落状态事件，forward 主机的时间线因此缺全部上线事件）
    crate::application::notification_service::record_online_and_notify(
        &deps.db,
        &deps.streams,
        host_id,
        hostname,
    )
    .await;
}

/// 阶段 6：回 RegisterAck。
///
/// 对端已断时 ack 无法送达：本次拨号会话随即失败退出，故此处**刻意不告警**——
/// 否则每次正常断连都会多一条无信息量的噪音（同 `send_msg` 的错误契约，T3）。
async fn ack_forward_register(tx: &mpsc::Sender<ServerMessage>) {
    let _ = tx
        .send(ServerMessage {
            kind: Some(server_message::Kind::RegisterAck(RegisterAck {
                ok: true,
                message: "registered".into(),
                heartbeat_interval_secs: 10,
            })),
        })
        .await;
}
