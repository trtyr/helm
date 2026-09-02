//! 实时流广播注册表：把 agent 上报的增量（日志/输出/指标）推给 WebSocket 订阅者。
//!
//! key 约定：`service:{id}`（服务日志）、`job:{id}`（job 输出）、`metrics`（全局指标）。

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};

/// 每个 key 的订阅者列表。
type Subscribers = Vec<mpsc::UnboundedSender<Vec<u8>>>;

/// 多订阅者广播表：key → 订阅者 sender 列表。
#[derive(Clone, Default)]
pub struct StreamRegistry {
    inner: Arc<Mutex<HashMap<String, Subscribers>>>,
}

impl StreamRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 订阅一个 key，返回接收端（WS 关闭时 drop receiver，broadcast 里 retain 清理）。
    pub async fn subscribe(&self, key: &str) -> mpsc::UnboundedReceiver<Vec<u8>> {
        let (tx, rx) = mpsc::unbounded_channel();
        self.inner
            .lock()
            .await
            .entry(key.to_string())
            .or_default()
            .push(tx);
        rx
    }

    /// 广播数据到 key 的所有订阅者（顺带清理失效 sender）。
    pub async fn broadcast(&self, key: &str, data: Vec<u8>) {
        if let Some(subs) = self.inner.lock().await.get_mut(key) {
            subs.retain(|tx| tx.send(data.clone()).is_ok());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn subscribe_broadcast_delivers() {
        let reg = StreamRegistry::new();
        let mut rx = reg.subscribe("job:1").await;
        reg.broadcast("job:1", b"hello".to_vec()).await;
        assert_eq!(rx.recv().await, Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn dropped_receiver_is_cleaned_up() {
        let reg = StreamRegistry::new();
        let rx = reg.subscribe("metrics").await;
        drop(rx);
        // broadcast 后 sender 应被 retain 清理，不再残留
        reg.broadcast("metrics", b"x".to_vec()).await;
        let inner = reg.inner.lock().await;
        let subs = inner.get("metrics").map(|v| v.len()).unwrap_or(0);
        assert_eq!(subs, 0);
    }
}
