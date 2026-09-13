//! 活跃连接注册表：管理 Agent 的在线连接与消息路由。
//!
//! 同 agent_id 重复注册（如同一 agent 重连、或同 id 多实例并存）时，新连接顶掉
//! 旧连接。旧连接必须显式踢下线：仅从表中移除旧发送端只会关闭下行方向，入站
//! 流任务仍在轮询，HTTP/2 流不复位，TCP 连接会永久残留（连接泄漏）。

use std::collections::HashMap;
use std::sync::Arc;

use helm_proto::pb::ServerMessage;
use tokio::sync::{Mutex, mpsc, watch};

/// 向 Agent 发送消息的失败原因。
#[derive(Debug, thiserror::Error)]
pub enum SendError {
    #[error("agent not connected: {0}")]
    NotConnected(String),
    #[error("connection closed: {0}")]
    Gone(String),
}

/// 被顶掉/注销的旧连接句柄：kick() 让其入站任务立即退出并释放 gRPC 流。
#[derive(Debug)]
pub struct ConnectionKick {
    kick: watch::Sender<bool>,
}

impl ConnectionKick {
    /// 通知该连接的入站任务退出。入站任务丢弃流后 HTTP/2 流被复位，
    /// 两端的 TCP 连接随之关闭。
    pub fn kick(&self) {
        let _ = self.kick.send(true);
    }
}

/// 注册返回的连接凭据。
pub struct RegisteredConnection {
    /// 本连接身份：on_disconnect 时用于判断本连接是否仍是该 agent 的当前注册项
    /// （被顶掉的旧连接不得注销新连接）。
    pub kick_tx: watch::Sender<bool>,
    /// 入站任务持有：wait_for 收到 kick 后立即退出。
    pub kick_rx: watch::Receiver<bool>,
    /// 注册前已存在的同 id 旧连接；应立即 kick，避免新旧连接并存。
    pub replaced: Option<ConnectionKick>,
}

/// 注册表中的一条连接。
struct ConnEntry {
    tx: mpsc::Sender<ServerMessage>,
    kick: watch::Sender<bool>,
}

/// 活跃连接注册表。每个在线 Agent 对应一个发送通道。
#[derive(Clone, Default)]
pub struct ConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, ConnEntry>>>,
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 Agent 连接；同 id 已有旧连接时将其顶掉并返回其踢下线句柄。
    pub async fn register(
        &self,
        agent_id: &str,
        tx: mpsc::Sender<ServerMessage>,
    ) -> RegisteredConnection {
        let (kick_tx, kick_rx) = watch::channel(false);
        let mut map = self.inner.lock().await;
        let replaced = map
            .insert(
                agent_id.to_string(),
                ConnEntry {
                    tx,
                    kick: kick_tx.clone(),
                },
            )
            .map(|old| ConnectionKick { kick: old.kick });
        RegisteredConnection {
            kick_tx,
            kick_rx,
            replaced,
        }
    }

    /// 强制注销一个 Agent 连接（不比对身份），返回被移除连接的踢下线句柄，
    /// 调用方应 kick 它以释放入站流。
    pub async fn unregister(&self, agent_id: &str) -> Option<ConnectionKick> {
        self.inner
            .lock()
            .await
            .remove(agent_id)
            .map(|old| ConnectionKick { kick: old.kick })
    }

    /// 仅当 `kick_tx` 仍是该 agent 的当前注册项时才注销，返回是否确实注销了。
    /// 连接正常断开走这里；被顶掉的旧连接断开时身份不匹配，不会误删新连接。
    pub async fn unregister_if_current(
        &self,
        agent_id: &str,
        kick_tx: &watch::Sender<bool>,
    ) -> bool {
        let mut map = self.inner.lock().await;
        if map
            .get(agent_id)
            .is_some_and(|e| e.kick.same_channel(kick_tx))
        {
            map.remove(agent_id);
            true
        } else {
            false
        }
    }

    /// 向指定 Agent 发送消息。
    pub async fn send(&self, agent_id: &str, msg: ServerMessage) -> Result<(), SendError> {
        let tx = self
            .inner
            .lock()
            .await
            .get(agent_id)
            .map(|e| e.tx.clone())
            .ok_or_else(|| SendError::NotConnected(agent_id.to_string()))?;
        tx.send(msg)
            .await
            .map_err(|_| SendError::Gone(agent_id.to_string()))
    }

    /// Agent 是否在线。
    pub async fn is_online(&self, agent_id: &str) -> bool {
        self.inner.lock().await.contains_key(agent_id)
    }

    /// 给定一组 agent_id，是否任一在线（单次加锁）。
    pub async fn any_online(&self, agent_ids: &[String]) -> bool {
        let map = self.inner.lock().await;
        agent_ids.iter().any(|id| map.contains_key(id))
    }

    /// 在线 Agent 数量。
    pub async fn online_count(&self) -> usize {
        self.inner.lock().await.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn register_send_unregister() {
        let reg = ConnectionRegistry::new();
        let (tx, mut rx) = mpsc::channel(4);
        let conn = reg.register("a1", tx).await;
        assert!(conn.replaced.is_none());
        assert!(reg.is_online("a1").await);
        assert_eq!(reg.online_count().await, 1);

        let msg = ServerMessage { kind: None };
        reg.send("a1", msg.clone()).await.unwrap();
        assert!(rx.recv().await.is_some());

        // 未连接 agent 发送失败
        assert!(matches!(
            reg.send("nope", msg).await,
            Err(SendError::NotConnected(_))
        ));

        assert!(reg.unregister("a1").await.is_some());
        assert!(!reg.is_online("a1").await);
        assert_eq!(reg.online_count().await, 0);
    }

    #[tokio::test]
    async fn duplicate_registration_replaces_and_kicks_old() {
        let reg = ConnectionRegistry::new();

        let (tx1, mut rx1) = mpsc::channel(4);
        let conn1 = reg.register("a1", tx1).await;
        assert!(conn1.replaced.is_none());

        // 同 id 二次注册：顶掉旧连接并返回踢下线句柄
        let (tx2, mut rx2) = mpsc::channel(4);
        let conn2 = reg.register("a1", tx2).await;
        assert!(conn2.replaced.is_some());
        conn2.replaced.as_ref().unwrap().kick();

        // 旧连接的入站任务能收到踢下线信号
        let mut old_rx = conn1.kick_rx;
        assert!(old_rx.wait_for(|k| *k).await.is_ok());

        // 路由走新连接；旧通道不再收到任何消息
        let msg = ServerMessage { kind: None };
        reg.send("a1", msg).await.unwrap();
        assert!(rx2.recv().await.is_some());
        assert!(rx1.try_recv().is_err());

        // 旧连接断开：身份不匹配，不得注销新连接
        assert!(!reg.unregister_if_current("a1", &conn1.kick_tx).await);
        assert!(reg.is_online("a1").await);

        // 当前连接断开：注销成功
        assert!(reg.unregister_if_current("a1", &conn2.kick_tx).await);
        assert!(!reg.is_online("a1").await);
    }
}
