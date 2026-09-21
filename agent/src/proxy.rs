//! 代理隧道数据面：收到 ProxyConnect 后从本机网络拨号目标，
//! 双向搬运 TCP 数据与 gRPC ProxyData 消息（正向/反向通道共用）。
//!
//! TCP 流按读写方向拆分：读半由回传任务独占（避免与下行写入争锁导致死锁），
//! 写半由管理器持有（tokio 异步锁），server 下行数据经 `data()` 写入。

use std::collections::HashMap;
use std::sync::Arc;

use helm_proto::pb::{AgentMessage, ProxyClose, ProxyConnected, ProxyData, agent_message};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;

/// 出站 TCP 写半（异步锁保护；读半由回传任务独占，二者互不阻塞）。
type SharedWriteHalf = Arc<tokio::sync::Mutex<tokio::net::tcp::OwnedWriteHalf>>;

/// 代理连接管理器：conn_id → 出站 TCP 写半。
#[derive(Clone, Default)]
pub struct ProxyManager {
    inner: Arc<std::sync::Mutex<HashMap<String, SharedWriteHalf>>>,
}

impl ProxyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 建立出站连接并回 ProxyConnected；成功后 spawn 读任务把 TCP 数据回传 server。
    pub async fn connect(&self, conn_id: &str, target: &str, tx: &mpsc::Sender<AgentMessage>) {
        tracing::info!(conn_id, target, "proxy: dialing target");
        let result = TcpStream::connect(target).await;
        match result {
            Ok(stream) => {
                let (read_half, write_half) = stream.into_split();
                let write_half = Arc::new(tokio::sync::Mutex::new(write_half));
                self.inner
                    .lock()
                    .unwrap()
                    .insert(conn_id.to_string(), write_half.clone());
                let _ = tx // peer 已断：投递失败无需上报（对端消失即会话结束）
                    .send(AgentMessage {
                        kind: Some(agent_message::Kind::ProxyConnected(ProxyConnected {
                            conn_id: conn_id.to_string(),
                            ok: true,
                            error: None,
                        })),
                    })
                    .await;
                // 读任务：独占读半，TCP → ProxyData 回传；EOF 时通知 server 关闭会话。
                let sid = conn_id.to_string();
                let tx2 = tx.clone();
                tokio::spawn(async move {
                    let mut buf = [0u8; 16384];
                    let mut reader = read_half;
                    loop {
                        match reader.read(&mut buf).await {
                            Ok(0) | Err(_) => break,
                            Ok(n) => {
                                let msg = AgentMessage {
                                    kind: Some(agent_message::Kind::ProxyData(ProxyData {
                                        conn_id: sid.clone(),
                                        data: buf[..n].to_vec(),
                                    })),
                                };
                                if tx2.send(msg).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    let _ = tx2 // peer 已断：投递失败无需上报（对端消失即会话结束）
                        .send(AgentMessage {
                            kind: Some(agent_message::Kind::ProxyClose(ProxyClose {
                                conn_id: sid,
                            })),
                        })
                        .await;
                });
                tracing::info!(conn_id, target, "proxy: tunnel established");
            }
            Err(e) => {
                tracing::warn!(conn_id, target, error = %e, "proxy: dial failed");
                let _ = tx // peer 已断：投递失败无需上报（对端消失即会话结束）
                    .send(AgentMessage {
                        kind: Some(agent_message::Kind::ProxyConnected(ProxyConnected {
                            conn_id: conn_id.to_string(),
                            ok: false,
                            error: Some(e.to_string()),
                        })),
                    })
                    .await;
            }
        }
    }

    /// server 方向的数据写入目标 TCP。
    pub async fn data(&self, conn_id: &str, data: &[u8]) {
        let writer = self.inner.lock().unwrap().get(conn_id).cloned();
        if let Some(w) = writer {
            let mut w = w.lock().await;
            let _ = w.write_all(data).await; // peer 已断：写失败即出站通道结束，接收方向会先感知并清理
            let _ = w.flush().await; // peer 已断：flush 失败无副作用（数据未送达即连接终止）
        }
    }

    /// 关闭连接（关写半触发对端 EOF，读任务随之结束）。
    pub async fn close(&self, conn_id: &str) {
        let w = self.inner.lock().unwrap().remove(conn_id);
        if let Some(w) = w {
            let _ = w.lock().await.shutdown().await; // 尽力优雅关闭（对端可能已消失），失败无副作用
        }
    }
}
