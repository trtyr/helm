//! 应用层：Agent 生命周期（列出 / 下线注销 / 卸载）。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::store::Db;
use crate::store::agent_repo::{AgentRepo, AgentRow};
use crate::store::host_repo::HostRepo;
use helm_proto::pb::{SelfDestruct, ServerMessage, server_message};

/// Agent 生命周期用例：下线 / 注销 / 卸载编排。
#[derive(Clone)]
pub struct AgentLifecycleService {
    db: Db,
    registry: ConnectionRegistry,
}

impl AgentLifecycleService {
    pub fn new(db: Db, registry: ConnectionRegistry) -> Self {
        Self { db, registry }
    }

    /// 列出所有 agent（附在线状态）。
    pub async fn list(&self) -> Result<Vec<AgentView>> {
        let agents = AgentRepo::new(self.db.clone()).list_all().await?;
        let mut views = Vec::with_capacity(agents.len());
        for agent in agents {
            let online = self.registry.is_online(&agent.id).await;
            views.push(AgentView { agent, online });
        }
        Ok(views)
    }

    /// 下线/注销：删除 agent 记录 + 孤儿 host 软删除，并解除连接。
    /// agent 离线时无法送达指令：删记录 + 标记挂起，重连瞬间补执行注销。
    pub async fn deregister(&self, agent_id: &str) -> Result<()> {
        let agent_repo = AgentRepo::new(self.db.clone());
        if !self.registry.is_online(agent_id).await {
            agent_repo
                .mark_pending_offline(agent_id, "deregister")
                .await?;
        }
        let host_id = agent_repo
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;

        agent_repo.delete(agent_id).await?;

        // 若该 host 已无其他 agent，软删除 host。
        let remaining = agent_repo.list_agent_ids(host_id).await?;
        if remaining.is_empty() {
            HostRepo::new(self.db.clone()).soft_delete(host_id).await?;
        }

        self.registry.unregister(agent_id).await;
        Ok(())
    }

    /// 卸载：下发 SelfDestruct 指令，随后注销 DB 记录。
    /// 返回是否即时送达；agent 离线时标记挂起（false），重连瞬间自动补下线。
    pub async fn uninstall(&self, agent_id: &str, remove_binary: bool) -> Result<bool> {
        if !self.registry.is_online(agent_id).await {
            AgentRepo::new(self.db.clone())
                .mark_pending_offline(agent_id, "uninstall")
                .await?;
            return Ok(false);
        }
        let msg = ServerMessage {
            kind: Some(server_message::Kind::SelfDestruct(SelfDestruct {
                remove_binary,
            })),
        };
        self.registry
            .send(agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;

        self.deregister(agent_id).await?;
        Ok(true)
    }
}

/// Agent 列表视图：附在线状态。
#[derive(Debug, serde::Serialize)]
pub struct AgentView {
    #[serde(flatten)]
    agent: AgentRow,
    online: bool,
}
