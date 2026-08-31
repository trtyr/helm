//! 活跃连接注册表：管理 Agent 的在线连接与消息路由。

use std::collections::HashMap;
use std::sync::Arc;

use helm_proto::pb::ServerMessage;
use tokio::sync::{Mutex, mpsc};

/// 向 Agent 发送消息的失败原因。
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("agent not connected: {0}")]
    NotConnected(String),
    #[error("connection closed: {0}")]
    Gone(String),
}

/// 活跃连接注册表。每个在线 Agent 对应一个发送通道。
#[derive(Clone, Default)]
pub struct ConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, mpsc::Sender<ServerMessage>>>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 Agent 连接。
    pub async fn register(&self, agent_id: &str, tx: mpsc::Sender<ServerMessage>) {
        self.inner.lock().await.insert(agent_id.to_string(), tx);
    }

    /// 注销一个 Agent 连接。
    pub async fn unregister(&self, agent_id: &str) {
        self.inner.lock().await.remove(agent_id);
    }

    /// 向指定 Agent 发送消息。
    pub async fn send(&self, agent_id: &str, msg: ServerMessage) -> Result<(), SendError> {
        let tx = self
            .inner
            .lock()
            .await
            .get(agent_id)
            .cloned()
            .ok_or_else(|| SendError::NotConnected(agent_id.to_string()))?;
        tx.send(msg)
            .await
            .map_err(|_| SendError::Gone(agent_id.to_string()))
    }

    /// Agent 是否在线。
    pub async fn is_online(&self, agent_id: &str) -> bool {
        self.inner.lock().await.contains_key(agent_id)
    }

    /// 在线 Agent 数量。
    pub async fn online_count(&self) -> usize {
        self.inner.lock().await.len()
    }
}
