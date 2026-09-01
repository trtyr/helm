//! 会话注册表：桥接 WebSocket ↔ Agent 的会话流。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

/// 会话注册表：session_id → 输出通道（转发到 WebSocket）。
#[derive(Clone, Default)]
pub struct SessionRegistry {
    inner: Arc<Mutex<HashMap<String, mpsc::UnboundedSender<Vec<u8>>>>>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册会话输出通道。
    pub async fn register(&self, session_id: &str, tx: mpsc::UnboundedSender<Vec<u8>>) {
        self.inner.lock().await.insert(session_id.to_string(), tx);
    }

    /// 注销会话。
    pub async fn unregister(&self, session_id: &str) {
        self.inner.lock().await.remove(session_id);
    }

    /// 转发 agent 输出到 WebSocket。返回会话是否存在。
    pub async fn forward(&self, session_id: &str, data: Vec<u8>) -> bool {
        let tx = self.inner.lock().await.get(session_id).cloned();
        match tx {
            Some(tx) => tx.send(data).is_ok(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn register_forward_unregister() {
        let reg = SessionRegistry::new();
        let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

        reg.register("s1", tx).await;
        assert!(reg.forward("s1", b"hello".to_vec()).await);
        assert_eq!(rx.recv().await, Some(b"hello".to_vec()));

        reg.unregister("s1").await;
        assert!(!reg.forward("s1", b"gone".to_vec()).await);
        // 未注册的会话 forward 返回 false
        assert!(!reg.forward("nope", b"x".to_vec()).await);
    }
}
