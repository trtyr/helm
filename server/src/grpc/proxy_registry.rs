//! 代理数据面注册表：桥接「SOCKS5 客户端 ↔ Agent gRPC 通道」的虚拟连接。
//!
//! 每条 conn_id 对应：ProxyConnected 应答（oneshot）+ server→客户端 数据下行通道
//! （unbounded）+ 客户端关闭通知（Notify）。

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use tokio::sync::{Mutex, Notify, mpsc, oneshot};

/// 代理连接建立结果。
pub type ConnectResult = Result<(), String>;

#[derive(Debug)]
pub struct ProxyEntry {
    pub data_tx: mpsc::UnboundedSender<Vec<u8>>,
    pub connected_tx: Option<oneshot::Sender<ConnectResult>>,
    pub client_gone: Arc<Notify>,
}

/// 待完成/活跃的代理连接注册表。
#[derive(Clone, Default)]
pub struct ProxyRegistry {
    inner: Arc<Mutex<HashMap<String, ProxyEntry>>>,
}

/// 全局单例（inbound 分发与 SOCKS 服务共用）。
pub fn registry() -> &'static ProxyRegistry {
    static R: OnceLock<ProxyRegistry> = OnceLock::new();
    R.get_or_init(ProxyRegistry::new)
}

impl ProxyRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一条待建立的代理连接。
    pub async fn register(
        &self,
        conn_id: String,
    ) -> (
        oneshot::Receiver<ConnectResult>,
        mpsc::UnboundedReceiver<Vec<u8>>,
        Arc<Notify>,
    ) {
        let (connected_tx, connected_rx) = oneshot::channel();
        let (data_tx, data_rx) = mpsc::unbounded_channel();
        let client_gone = Arc::new(Notify::new());
        self.inner.lock().await.insert(
            conn_id,
            ProxyEntry {
                data_tx,
                connected_tx: Some(connected_tx),
                client_gone: client_gone.clone(),
            },
        );
        (connected_rx, data_rx, client_gone)
    }

    /// Agent 报告连接建立结果（oneshot 只消费一次，条目保留供后续数据路由）。
    pub async fn connected(&self, conn_id: &str, ok: bool, error: Option<String>) {
        if let Some(entry) = self.inner.lock().await.get_mut(conn_id) {
            let result = if ok {
                Ok(())
            } else {
                Err(error.unwrap_or_default())
            };
            if let Some(tx) = entry.connected_tx.take() {
                let _ = tx.send(result); // 等待方（客户端泵）可能已退出：oneshot 送达失败无副作用
            }
        }
    }

    /// Agent → 客户端 数据块。
    pub async fn data(&self, conn_id: &str, data: Vec<u8>) {
        if let Some(entry) = self.inner.lock().await.get(conn_id) {
            let _ = entry.data_tx.send(data); // 客户端泵已退出则丢弃该块（该连接已结束）
        }
    }

    /// Agent 侧关闭 → 通知客户端泵退出。
    pub async fn closed(&self, conn_id: &str) {
        if let Some(entry) = self.inner.lock().await.remove(conn_id) {
            entry.client_gone.notify_waiters();
        }
    }

    /// 客户端侧主动注销（断开/结束时调用）。
    pub async fn unregister(&self, conn_id: &str) {
        self.inner.lock().await.remove(conn_id);
    }

    /// Agent 断连：关闭其名下全部代理连接。由调用方逐 conn 触发。
    pub async fn closed_by_ids(&self, conn_ids: &[String]) {
        for id in conn_ids {
            self.closed(id).await;
        }
    }
}
