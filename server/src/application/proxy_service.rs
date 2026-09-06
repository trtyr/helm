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
            let _ = task.await;
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
async fn socks5_client(
    mut socket: tokio::net::TcpStream,
    agent_id: String,
    conn_registry: ConnectionRegistry,
) {
    // 1. 握手：VER(5) NMETHODS METHODS... → 选 NO AUTH
    let mut head = [0u8; 2];
    if socket.read_exact(&mut head).await.is_err() || head[0] != 5 {
        return;
    }
    let mut methods = vec![0u8; head[1] as usize];
    if socket.read_exact(&mut methods).await.is_err() {
        return;
    }
    if socket.write_all(&[5, 0]).await.is_err() {
        return;
    }
    tracing::debug!("socks5: handshake replied");

    // 2. 请求：VER CMD RSV ATYP ADDR PORT（只支持 CONNECT=1）
    let mut req_head = [0u8; 4];
    if socket.read_exact(&mut req_head).await.is_err() || req_head[1] != 1 {
        let _ = socket.write_all(&[5, 7, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return;
    }
    let target = match read_socks_addr(&mut socket, req_head[3]).await {
        Some(t) => {
            tracing::debug!(agent_id = %agent_id, %t, "socks5: CONNECT target parsed");
            t
        }
        None => {
            let _ = socket.write_all(&[5, 8, 0, 1, 0, 0, 0, 0, 0, 0]).await;
            return;
        }
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
        let _ = socket.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).await;
        return;
    }

    // 4. SOCKS 成功应答（BND 填零）
    if socket
        .write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])
        .await
        .is_err()
    {
        tracing::warn!(agent_id = %agent_id, "socks5: success reply write failed");
        return;
    }
    tracing::debug!(agent_id = %agent_id, "socks5: success reply sent, entering pump");

    // 5. 双向泵：socket ↔ (ProxyData 经 gRPC 下行 / data_rx 上行)
    let (mut sock_rx, mut sock_tx) = socket.split();
    let up = async {
        let mut buf = [0u8; 16384];
        loop {
            match sock_rx.read(&mut buf).await {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    tracing::debug!(n, "socks5: up pump got client bytes");
                    let msg = ServerMessage {
                        kind: Some(server_message::Kind::ProxyData(ProxyData {
                            conn_id: conn_id.clone(),
                            data: buf[..n].to_vec(),
                        })),
                    };
                    if conn_registry.send(agent_id.as_str(), msg).await.is_err() {
                        break;
                    }
                }
            }
        }
    };
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
    let _ = conn_registry.send(agent_id.as_str(), msg).await;
    let _ = sock_tx.shutdown().await;
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
