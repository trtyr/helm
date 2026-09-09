//! forward 模式持久连接管理器。
//!
//! 为 `conn_mode='forward'` 且 `addr` 非空的 host 维护 Server 主动拨号的持久双向流：
//! 流上协议与 reverse 完全同构（agent 先发 Register，随后心跳/指标/结果上报），
//! 注册进 ConnectionRegistry 后，全部控制端点（terminal / files / services /
//! processes / net / exec）对 forward 主机可用。
//!
//! 连接生命周期：`spawn_reconciler` 周期性对照 hosts 表差分启停拨号循环；
//! 拨号失败或流断开自动重连；host 删除或 addr 变更时停旧起新。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::application::cert_service::CertService;
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::inbound::InboundCtx;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use crate::store::agent_repo::AgentRepo;
use helm_proto::pb::{
    RegisterAck, ServerMessage, agent_message,
    forward_agent_service_client::ForwardAgentServiceClient, server_message,
};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Certificate, ClientTlsConfig, Identity};
use uuid::Uuid;

/// 拨号失败后的重连间隔。
const RECONNECT_DELAY: Duration = Duration::from_secs(5);
/// 与 hosts 表差分的周期。
const RECONCILE_INTERVAL: Duration = Duration::from_secs(10);

/// 拨号循环任务句柄：addr + 停止信号发送端。
type DialTask = (String, mpsc::Sender<()>);

/// 拨号循环依赖集合（Clone 便宜，全部是 Arc/句柄）。
#[derive(Clone)]
pub struct ForwardDeps {
    pub registry: ConnectionRegistry,
    pub transfers: TransferRegistry,
    pub sessions: SessionRegistry,
    pub file_list: FileListRegistry,
    pub query: QueryRegistry,
    pub streams: StreamRegistry,
    pub db: Db,
    pub server_token: String,
    /// mTLS 证书服务（enabled 时拨号走 TLS 双向认证）
    pub cert: CertService,
    /// 校验 agent 证书 SAN 用的 server name
    pub tls_server_name: String,
}

/// forward 持久连接管理器：host_id → (拨号 addr, 停止信号发送端)。
#[derive(Clone, Default)]
pub struct ForwardManager {
    tasks: Arc<Mutex<HashMap<Uuid, DialTask>>>,
}

impl ForwardManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 确保某 host 的拨号循环在跑（幂等；已在跑则跳过）。
    pub async fn ensure(&self, host_id: Uuid, addr: String, deps: ForwardDeps) {
        let mut tasks = self.tasks.lock().await;
        if tasks.contains_key(&host_id) {
            return;
        }
        let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
        tasks.insert(host_id, (addr.clone(), stop_tx));
        drop(tasks);
        tracing::info!(host_id = %host_id, addr = %addr, "forward dial loop started");
        tokio::spawn(run_host_loop(host_id, addr, deps, stop_rx));
    }

    /// 停止某 host 的拨号循环（并断开当前连接）。
    pub async fn remove(&self, host_id: &Uuid) {
        let tx = self.tasks.lock().await.remove(host_id).map(|(_, tx)| tx);
        if let Some(tx) = tx {
            let _ = tx.send(()).await;
            tracing::info!(host_id = %host_id, "forward dial loop stopped");
        }
    }

    /// 与 hosts 表差分启停（host 新增/addr 变更 → 起新循环；删除 → 停旧循环）。
    pub async fn reconcile(&self, deps: &ForwardDeps) {
        let rows: Vec<(Uuid, String)> = sqlx::query_as(
            "SELECT id, addr FROM hosts
             WHERE conn_mode = 'forward' AND addr <> '' AND deleted_at IS NULL",
        )
        .fetch_all(deps.db.pool())
        .await
        .unwrap_or_default();
        let desired: HashMap<Uuid, String> = rows.into_iter().collect();

        let running: HashMap<Uuid, String> = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .map(|(id, (addr, _))| (*id, addr.clone()))
                .collect()
        };
        let (to_start, to_stop) = diff_hosts(&running, &desired);

        for (id, addr) in to_start {
            self.ensure(id, addr, deps.clone()).await;
        }
        for id in to_stop {
            self.remove(&id).await;
        }
    }

    /// 后台 reconcile 循环。
    pub fn spawn_reconciler(&self, deps: ForwardDeps) {
        let mgr = self.clone();
        tokio::spawn(async move {
            loop {
                mgr.reconcile(&deps).await;
                tokio::time::sleep(RECONCILE_INTERVAL).await;
            }
        });
    }
}

/// 单个 forward host 的持久拨号循环：断线重连，直到收到停止信号。
async fn run_host_loop(
    host_id: Uuid,
    addr: String,
    deps: ForwardDeps,
    mut stop_rx: mpsc::Receiver<()>,
) {
    loop {
        // connect_once 内部的入站循环会响应停止信号
        let res = connect_once(host_id, &addr, &deps, &mut stop_rx).await;
        if let Err(e) = res {
            tracing::warn!(host_id = %host_id, addr = %addr, error = %e, "forward connect failed");
        }
        // 重连延迟（同样可被停止信号打断）
        tokio::select! {
            _ = stop_rx.recv() => break,
            _ = tokio::time::sleep(RECONNECT_DELAY) => {}
        }
    }
}

/// 拨号一次：建流 → 等 agent Register → 校验 token → 落库注册 → 消费入站流。
async fn connect_once(
    host_id: Uuid,
    addr: &str,
    deps: &ForwardDeps,
    stop_rx: &mut mpsc::Receiver<()>,
) -> anyhow::Result<()> {
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
        let tls = ClientTlsConfig::new()
            .domain_name(deps.tls_server_name.clone())
            .ca_certificate(Certificate::from_pem(deps.cert.ca_cert_pem().as_bytes()))
            .identity(Identity::from_pem(
                deps.cert.server_cert_pem().as_bytes(),
                deps.cert.server_key_pem().as_bytes(),
            ));
        endpoint = endpoint.tls_config(tls)?;
    }
    let channel = endpoint.connect().await?;
    let mut client = ForwardAgentServiceClient::new(channel);

    let (tx, rx) = mpsc::channel::<ServerMessage>(64);
    let response = client.open_forward_channel(ReceiverStream::new(rx)).await?;
    let mut inbound = response.into_inner();

    // 首条消息必须是 agent 主动发的 Register
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| anyhow::anyhow!("channel closed before Register"))?;
    let register = match first.kind {
        Some(agent_message::Kind::Register(r)) => r,
        _ => anyhow::bail!("first message must be Register"),
    };
    if !crate::grpc::agent_service::token_matches(&deps.server_token, &register.token) {
        anyhow::bail!("invalid token");
    }
    let agent_id = register.agent_id.clone();

    // 落库：挂到声明的 forward host 下（不新建 host）。
    // public_ip 取拨号地址的 IP 部分（forward 场景 Server 主动拨出，该地址即对外可达地址）。
    let host_info = register.host.clone().unwrap_or_default();
    let public_ip = addr.split(':').next().unwrap_or_default().to_string();
    AgentRepo::new(deps.db.clone())
        .register_under_host(
            host_id,
            &agent_id,
            &register.version,
            &host_info.os,
            &host_info.arch,
            &host_info.platform,
            &public_ip,
            &host_info.local_ips,
            host_info.elevated,
        )
        .await?;

    let registration = deps.registry.register(&agent_id, tx.clone()).await;
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

    // 上线通知（决策 009：forward 与 reverse 同构；重复注册仅换连接，不通知）
    if registration.replaced.is_none() {
        let hostname = host_info.hostname.clone();
        let svc = crate::application::notification_service::NotificationService::new(
            deps.db.clone(),
            deps.streams.clone(),
        );
        if let Err(e) = svc
            .notify(
                host_id,
                crate::application::notification_service::KIND_ONLINE,
                &format!("主机 {hostname} 已上线"),
            )
            .await
        {
            tracing::warn!(agent_id = %agent_id, error = ?e, "online notify failed");
        }
    }

    // 回 RegisterAck
    let _ = tx
        .send(ServerMessage {
            kind: Some(server_message::Kind::RegisterAck(RegisterAck {
                ok: true,
                message: "registered".into(),
                heartbeat_interval_secs: 10,
            })),
        })
        .await;

    // 入站循环（与 reverse 同构），stop 信号立即断开
    let register_hostname = host_info.hostname.clone();
    let mut ctx = InboundCtx::new(
        agent_id.clone(),
        Some(host_id),
        register_hostname,
        deps.registry.clone(),
        registration.kick_tx,
        deps.transfers.clone(),
        deps.sessions.clone(),
        deps.file_list.clone(),
        deps.query.clone(),
        deps.streams.clone(),
        deps.db.clone(),
    );
    let mut kick_rx = registration.kick_rx;
    loop {
        tokio::select! {
            _ = stop_rx.recv() => break,
            // 被同 id 新注册顶掉：立即退出释放流（watch::Ref 非 Send，包 async 块丢弃）
            _ = async {
                let _ = kick_rx.wait_for(|kicked| *kicked).await;
            } => break,
            msg = inbound.message() => {
                match msg {
                    Ok(Some(m)) => ctx.handle(m).await,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(agent_id = %agent_id, error = %e, "forward inbound stream error");
                        break;
                    }
                }
            }
        }
    }
    ctx.on_disconnect().await;
    anyhow::bail!("stream closed")
}

/// 差分：返回 (应启动的 (host_id, addr)，应停止的 host_id)。
/// running 值为当前 addr（空串表示 addr 未知），addr 变更视为重启。
fn diff_hosts(
    running: &HashMap<Uuid, String>,
    desired: &HashMap<Uuid, String>,
) -> (Vec<(Uuid, String)>, Vec<Uuid>) {
    let to_start: Vec<(Uuid, String)> = desired
        .iter()
        .filter(|(id, addr)| {
            running
                .get(id)
                .map(|r| r.is_empty() || r != *addr)
                .unwrap_or(true)
        })
        .map(|(id, addr)| (*id, addr.clone()))
        .collect();
    let to_stop: Vec<Uuid> = running
        .iter()
        .filter(|(id, r)| desired.get(id).map(|d| d != *r).unwrap_or(true))
        .map(|(id, _)| *id)
        .collect();
    (to_start, to_stop)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(Uuid, &str)]) -> HashMap<Uuid, String> {
        pairs.iter().map(|(id, a)| (*id, a.to_string())).collect()
    }

    #[test]
    fn diff_starts_new_and_stops_removed() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let running = map(&[(a, "1.1.1.1:1")]);
        let desired = map(&[(b, "2.2.2.2:2")]);
        let (start, stop) = diff_hosts(&running, &desired);
        assert_eq!(start, vec![(b, "2.2.2.2:2".to_string())]);
        assert_eq!(stop, vec![a]);
    }

    #[test]
    fn diff_addr_change_restarts() {
        let a = Uuid::new_v4();
        let running = map(&[(a, "1.1.1.1:1")]);
        let desired = map(&[(a, "1.1.1.1:2")]);
        let (start, stop) = diff_hosts(&running, &desired);
        assert_eq!(start, vec![(a, "1.1.1.1:2".to_string())]);
        assert_eq!(stop, vec![a]);
    }

    #[test]
    fn diff_no_change_is_noop() {
        let a = Uuid::new_v4();
        let running = map(&[(a, "1.1.1.1:1")]);
        let desired = map(&[(a, "1.1.1.1:1")]);
        let (start, stop) = diff_hosts(&running, &desired);
        assert!(start.is_empty());
        assert!(stop.is_empty());
    }

    #[test]
    fn diff_unknown_running_addr_restarts() {
        // addr 未知（空串）视为需要重启
        let a = Uuid::new_v4();
        let running = map(&[(a, "")]);
        let desired = map(&[(a, "1.1.1.1:1")]);
        let (start, stop) = diff_hosts(&running, &desired);
        assert_eq!(start, vec![(a, "1.1.1.1:1".to_string())]);
        assert_eq!(stop, vec![a]);
    }
}
