//! 应用层：SOCKS5 代理服务。本地监听 SOCKS5 端口，CONNECT 的目标连接
//! 经 gRPC 通道转发给 Agent 从目标机网络拨号，实现以目标机为出口的网络跳板。

use std::collections::HashMap;
use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::{Mutex, Notify};
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::proxy_registry::registry;
use helm_proto::pb::{ProxyClose, ProxyConnect, ProxyData, ServerMessage, server_message};

/// 一条活跃代理实例（agent + SOCKS5 监听）。
pub struct ProxyInstance {
    pub agent_id: String,
    pub listen_addr: String,
    stop: Arc<Notify>,
    task: Mutex<Option<JoinHandle<()>>>,
}

/// 代理服务管理器。
#[derive(Clone)]
pub struct ProxyService {
    inner: Arc<Mutex<HashMap<Uuid, Arc<ProxyInstance>>>>,
}

impl Default for ProxyService {
    fn default() -> Self {
        Self::new()
    }
}

impl ProxyService {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 为 agent 开启 SOCKS5 监听。端口占用时返回错误。
    pub async fn start(
        &self,
        agent_id: &str,
        listen_addr: &str,
        conn_registry: ConnectionRegistry,
    ) -> Result<(Uuid, String)> {
        let listener = TcpListener::bind(listen_addr)
            .await
            .map_err(|e| Error::InvalidArgument(format!("SOCKS5 监听 {listen_addr} 失败: {e}")))?;
        let actual = listener
            .local_addr()
            .map_err(|e| Error::Internal(e.to_string()))?
            .to_string();

        let id = Uuid::new_v4();
        let stop = Arc::new(Notify::new());
        let agent = agent_id.to_string();

        // 接受循环：每个客户端一个任务；stop 信号触发时整体退出。
        let accept_task = {
            let agent = agent.clone();
            let conn_registry = conn_registry.clone();
            let stop = stop.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        _ = stop.notified() => break,
                        accepted = listener.accept() => {
                            match accepted {
                                Ok((socket, _peer)) => {
                                    let agent = agent.clone();
                                    let conn_registry = conn_registry.clone();
                                    let stop = stop.clone();
                                    tokio::spawn(async move {
                                        tokio::select! {
                                            _ = stop.notified() => {}
                                            _ = socks5_client(socket, agent, conn_registry) => {}
                                        }
                                    });
                                }
                                Err(_) => break,
                            }
                        }
                    }
                }
            })
        };

        let instance = Arc::new(ProxyInstance {
            agent_id: agent_id.to_string(),
            listen_addr: actual.clone(),
            stop,
            task: Mutex::new(Some(accept_task)),
        });
        self.inner.lock().await.insert(id, instance);
        Ok((id, actual))
    }

    /// 停止代理：关闭监听与全部活跃连接。
    pub async fn stop(&self, id: Uuid) -> Result<()> {
        let instance = self
            .inner
            .lock()
            .await
            .remove(&id)
            .ok_or_else(|| Error::NotFound(format!("proxy: {id}")))?;
        instance.stop.notify_waiters();
        if let Some(task) = instance.task.lock().await.take() {
            // 代理会话任务结束即代理终止；JoinError（panic/取消）必须可见
            if let Err(e) = task.await {
                tracing::warn!(proxy_id = %id, error = %e, "proxy session task ended abnormally");
            }
        }
        Ok(())
    }

    /// 列出全部活跃代理。
    pub async fn list(&self) -> Vec<(Uuid, String, String)> {
        self.inner
            .lock()
            .await
            .iter()
            .map(|(id, p)| (*id, p.agent_id.clone(), p.listen_addr.clone()))
            .collect()
    }
}

/// 处理单个 SOCKS5 客户端：握手 → CONNECT → 经 agent 隧道双向搬运。
///
/// **G11 拆分（2026-09-21）**：原为 122 行单块。前两阶段抽为 `socks5_handshake`
/// （方法协商）与 `socks5_request`（CONNECT 请求解析，含 REP 回包）；本函数保留
/// 「拨号 → 成功应答 → 双向泵 → 回收」这条主链，因为其中每一段都与本地 socket
/// 生命周期强耦合（半关闭、teardown 必须在同一处收口）。
async fn socks5_client(
    mut socket: tokio::net::TcpStream,
    agent_id: String,
    conn_registry: ConnectionRegistry,
) {
    // 阶段 1：握手（VER=5 + 选 NO AUTH）
    if !socks5_handshake(&mut socket).await {
        return;
    }

    // 阶段 2：请求（VER CMD RSV ATYP ADDR PORT，只支持 CONNECT=1）
    let Some(target) = socks5_request(&mut socket, &agent_id).await else {
        return;
    };

    // 3. 经 agent 隧道拨号（15s 超时）
    let conn_id = uuid::Uuid::new_v4().to_string();
    let (connected_rx, mut data_rx, client_gone) = registry().register(conn_id.clone()).await;
    let msg = ServerMessage {
        kind: Some(server_message::Kind::ProxyConnect(ProxyConnect {
            conn_id: conn_id.clone(),
            target: target.clone(),
        })),
    };
    if conn_registry.send(agent_id.as_str(), msg).await.is_err() {
        return;
    }
    let connect_result =
        match tokio::time::timeout(std::time::Duration::from_secs(15), connected_rx).await {
            Ok(Ok(Ok(()))) => {
                tracing::debug!(agent_id = %agent_id, "socks5: agent dial ok");
                Ok(())
            }
            Ok(Ok(Err(e))) => {
                tracing::warn!(agent_id = %agent_id, error = %e, "socks5: agent dial failed");
                Err(())
            }
            _ => {
                tracing::warn!(agent_id = %agent_id, "socks5: connect wait timeout/closed");
                Err(())
            }
        };
    if connect_result.is_err() {
        // REP=5 connection refused
        let _ = write_socks_reply(&mut socket, 5).await; // 对端已断：连接失败回复写不出去即会话终止
        return;
    }

    // 阶段 4：SOCKS 成功应答（BND 填零）
    if write_socks_reply(&mut socket, 0).await.is_err() {
        tracing::warn!(agent_id = %agent_id, "socks5: success reply write failed");
        return;
    }
    tracing::debug!(agent_id = %agent_id, "socks5: success reply sent, entering pump");

    // 阶段 5：双向泵：socket ↔ (ProxyData 经 gRPC 下行 / data_rx 上行)
    let (mut sock_rx, mut sock_tx) = socket.split();
    let up = pump_up(&mut sock_rx, &conn_registry, &agent_id, &conn_id);
    let down = async {
        while let Some(chunk) = data_rx.recv().await {
            if sock_tx.write_all(&chunk).await.is_err() {
                break;
            }
        }
    };
    tokio::select! {
        _ = up => {}
        _ = down => {}
        _ = client_gone.notified() => {}
    }
    registry().unregister(&conn_id).await;
    // 通知 agent 关闭出站连接
    let msg = ServerMessage {
        kind: Some(server_message::Kind::ProxyClose(ProxyClose { conn_id })),
    };
    let _ = conn_registry.send(agent_id.as_str(), msg).await; // agent 已断：ProxyClose 无需送达，本地资源照常回收
    let _ = sock_tx.shutdown().await; // 尽力关闭（对端可能已消失），失败无副作用
}

/// 回 SOCKS5 应答帧（VER=5 + REP + RSV=0 + ATYP=1 + BND=0.0.0.0:0）。
/// 写失败即对端已断——调用方据此终止会话。
async fn write_socks_reply(socket: &mut tokio::net::TcpStream, rep: u8) -> std::io::Result<()> {
    socket.write_all(&[5, rep, 0, 1, 0, 0, 0, 0, 0, 0]).await
}

/// 上行泵：客户端 → agent（ProxyData 经 gRPC 下行）；读尽或发送失败即结束。
///
/// 它与下行泵赛跑（`select!`），因此**内部不做任何清理动作**：半关闭与
/// `ProxyClose` 由调用方在 select 之后统一处理，避免取消时留下半成品状态。
async fn pump_up(
    sock_rx: &mut (impl tokio::io::AsyncRead + Unpin),
    conn_registry: &ConnectionRegistry,
    agent_id: &str,
    conn_id: &str,
) {
    let mut buf = [0u8; 16384];
    loop {
        match sock_rx.read(&mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                tracing::debug!(n, "socks5: up pump got client bytes");
                let msg = ServerMessage {
                    kind: Some(server_message::Kind::ProxyData(ProxyData {
                        conn_id: conn_id.to_string(),
                        data: buf[..n].to_vec(),
                    })),
                };
                if conn_registry.send(agent_id, msg).await.is_err() {
                    break;
                }
            }
        }
    }
}

/// 阶段 1：SOCKS5 方法协商——VER(5) NMETHODS METHODS... → 回 NO AUTH。
/// 版本不符或读写失败返回 `false`（会话终止）。
async fn socks5_handshake(socket: &mut tokio::net::TcpStream) -> bool {
    let mut head = [0u8; 2];
    if socket.read_exact(&mut head).await.is_err() || head[0] != 5 {
        return false;
    }
    let mut methods = vec![0u8; head[1] as usize];
    if socket.read_exact(&mut methods).await.is_err() {
        return false;
    }
    if socket.write_all(&[5, 0]).await.is_err() {
        return false;
    }
    tracing::debug!("socks5: handshake replied");
    true
}

/// 阶段 2：读 CONNECT 请求（VER CMD RSV ATYP ADDR PORT），只支持 CMD=1。
///
/// 失败时自行回错误应答（REP=7 命令不支持 / REP=8 地址类型不支持）并返回 `None`；
/// 两条 `write_all` 失败即对端已断，会话终止，无需额外告警。
async fn socks5_request(socket: &mut tokio::net::TcpStream, agent_id: &str) -> Option<String> {
    let mut req_head = [0u8; 4];
    if socket.read_exact(&mut req_head).await.is_err() || req_head[1] != 1 {
        // 对端已断：错误回复写不出去即会话终止
        let _ = socket.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return None;
    }
    match read_socks_addr(socket, req_head[3]).await {
        Some(t) => {
            tracing::debug!(agent_id = %agent_id, %t, "socks5: CONNECT target parsed");
            Some(t)
        }
        None => {
            // 对端已断：地址类型不支持回复写不出去即会话终止
            let _ = socket.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            None
        }
    }
}

/// 解析 SOCKS 地址字段（ATYP + ADDR + PORT）→ "host:port"。
async fn read_socks_addr(
    socket: &mut (impl tokio::io::AsyncRead + Unpin),
    atyp: u8,
) -> Option<String> {
    match atyp {
        1 => {
            let mut b = [0u8; 6]; // IPv4(4) + port(2)
            socket.read_exact(&mut b).await.ok()?;
            let ip = std::net::Ipv4Addr::new(b[0], b[1], b[2], b[3]);
            Some(format!("{}:{}", ip, u16::from_be_bytes([b[4], b[5]])))
        }
        3 => {
            let mut len = [0u8; 1];
            socket.read_exact(&mut len).await.ok()?;
            let mut domain = vec![0u8; len[0] as usize];
            socket.read_exact(&mut domain).await.ok()?;
            let mut pb = [0u8; 2];
            socket.read_exact(&mut pb).await.ok()?;
            Some(format!(
                "{}:{}",
                String::from_utf8_lossy(&domain),
                u16::from_be_bytes([pb[0], pb[1]])
            ))
        }
        4 => {
            let mut b = [0u8; 18]; // IPv6(16) + port(2)
            socket.read_exact(&mut b).await.ok()?;
            let ip = std::net::Ipv6Addr::new(
                u16::from_be_bytes([b[0], b[1]]),
                u16::from_be_bytes([b[2], b[3]]),
                u16::from_be_bytes([b[4], b[5]]),
                u16::from_be_bytes([b[6], b[7]]),
                u16::from_be_bytes([b[8], b[9]]),
                u16::from_be_bytes([b[10], b[11]]),
                u16::from_be_bytes([b[12], b[13]]),
                u16::from_be_bytes([b[14], b[15]]),
            );
            Some(format!("[{ip}]:{}", u16::from_be_bytes([b[16], b[17]])))
        }
        _ => None,
    }
}
