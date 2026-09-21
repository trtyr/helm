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
    /// 下行通道满且在超时窗口内未被消费（agent 挂起/停止读取），按失联处理（E1）。
    #[error("send timeout: {0}")]
    Timeout(String),
}

/// 注册失败原因（E3：注册表容量上限）。
#[derive(Debug, thiserror::Error)]
pub enum RegisterError {
    #[error("connection registry full ({0} agents), rejecting new registration")]
    Full(usize),
}

/// 默认注册表容量上限（E3）：防失控 agent 注册潮拖垮内存。
pub const DEFAULT_MAX_AGENTS: usize = 1024;

/// 下行发送超时（E1）：通道满超过此时长即视为 agent 失联。
pub const SEND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// 被顶掉/注销的旧连接句柄：kick() 让其入站任务立即退出并释放 gRPC 流。
#[derive(Debug)]
pub struct ConnectionKick {
    kick: watch::Sender<bool>,
}

impl ConnectionKick {
    /// 通知该连接的入站任务退出。入站任务丢弃流后 HTTP/2 流被复位，
    /// 两端的 TCP 连接随之关闭。
    pub fn kick(&self) {
        let _ = self.kick.send(true); // watch 通道：接收侧已释放即无需 kick（该连接本就已结束）
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
#[derive(Clone)]
pub struct ConnectionRegistry {
    inner: Arc<Mutex<HashMap<String, ConnEntry>>>,
    /// 注册容量上限（E3）；同 id 重连（顶号）不受此限。
    max_agents: usize,
}

impl Default for ConnectionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnectionRegistry {
    pub fn new() -> Self {
        Self::with_max_agents(DEFAULT_MAX_AGENTS)
    }

    /// 指定容量上限构造（测试用小值验证拒绝语义）。
    pub fn with_max_agents(max_agents: usize) -> Self {
        Self {
            inner: Arc::default(),
            max_agents: max_agents.max(1),
        }
    }

    /// 注册一个 Agent 连接；同 id 已有旧连接时将其顶掉并返回其踢下线句柄。
    ///
    /// E3：容量达上限时拒绝**新** agent 注册（同 id 重连不受限）。
    pub async fn register(
        &self,
        agent_id: &str,
        tx: mpsc::Sender<ServerMessage>,
    ) -> Result<RegisteredConnection, RegisterError> {
        let (kick_tx, kick_rx) = watch::channel(false);
        let mut map = self.inner.lock().await;
        if !map.contains_key(agent_id) && map.len() >= self.max_agents {
            return Err(RegisterError::Full(self.max_agents));
        }
        let replaced = map
            .insert(
                agent_id.to_string(),
                ConnEntry {
                    tx,
                    kick: kick_tx.clone(),
                },
            )
            .map(|old| ConnectionKick { kick: old.kick });
        Ok(RegisteredConnection {
            kick_tx,
            kick_rx,
            replaced,
        })
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

    /// 向指定 Agent 发送消息（默认 5s 超时，E1）。
    pub async fn send(&self, agent_id: &str, msg: ServerMessage) -> Result<(), SendError> {
        self.send_with_timeout(agent_id, msg, SEND_TIMEOUT).await
    }

    /// 向指定 Agent 发送消息，带显式超时（E1：通道满超时即按失联处理，
    /// 不引入 try_send 行为突变——正常情况下语义与无界等待一致）。
    pub async fn send_with_timeout(
        &self,
        agent_id: &str,
        msg: ServerMessage,
        timeout: std::time::Duration,
    ) -> Result<(), SendError> {
        let tx = self
            .inner
            .lock()
            .await
            .get(agent_id)
            .map(|e| e.tx.clone())
            .ok_or_else(|| SendError::NotConnected(agent_id.to_string()))?;
        // 锁已释放，超时不会阻塞其他 agent 的路由
        tokio::time::timeout(timeout, tx.send(msg))
            .await
            .map_err(|_| SendError::Timeout(agent_id.to_string()))?
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
        let conn = reg.register("a1", tx).await.unwrap();
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
        let conn1 = reg.register("a1", tx1).await.unwrap();
        assert!(conn1.replaced.is_none());

        // 同 id 二次注册：顶掉旧连接并返回踢下线句柄
        let (tx2, mut rx2) = mpsc::channel(4);
        let conn2 = reg.register("a1", tx2).await.unwrap();
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

    #[tokio::test]
    async fn send_times_out_when_downstream_never_consumed() {
        // E1：容量 1 的通道塞满且无人消费 → send_with_timeout 超时返回 Timeout
        let reg = ConnectionRegistry::new();
        let (tx, rx) = mpsc::channel(1);
        reg.register("a1", tx).await.unwrap();
        // 占满通道
        let _permit = rx; // 消费端持有不读
        reg.send("a1", ServerMessage { kind: None }).await.unwrap();
        // 第二条必然阻塞 → 50ms 超时
        let started = std::time::Instant::now();
        let result = reg
            .send_with_timeout(
                "a1",
                ServerMessage { kind: None },
                std::time::Duration::from_millis(50),
            )
            .await;
        assert!(
            matches!(result, Err(SendError::Timeout(_))),
            "expected Timeout, got {result:?}"
        );
        assert!(started.elapsed() >= std::time::Duration::from_millis(45));
    }

    #[tokio::test]
    async fn register_rejects_new_agents_at_capacity_but_allows_rebind() {
        // E3：上限 2 → 第 3 个新 agent 拒绝；已有 id 重连（顶号）不受限
        let reg = ConnectionRegistry::with_max_agents(2);
        let (tx1, _rx1) = mpsc::channel(1);
        let (tx2, _rx2) = mpsc::channel(1);
        reg.register("a", tx1).await.unwrap();
        reg.register("b", tx2).await.unwrap();

        let (tx3, _rx3) = mpsc::channel(1);
        assert!(
            matches!(reg.register("c", tx3).await, Err(RegisterError::Full(2))),
            "new agent must be rejected at capacity"
        );

        // 同 id 重连不受上限约束（agent 重连是恢复路径，必须放行）
        let (tx2b, mut rx2b) = mpsc::channel(1);
        let rebind = reg.register("b", tx2b).await.unwrap();
        assert!(rebind.replaced.is_some(), "rebind must kick old connection");
        assert_eq!(reg.online_count().await, 2);
        reg.send("b", ServerMessage { kind: None }).await.unwrap();
        assert!(rx2b.recv().await.is_some());
    }
}
