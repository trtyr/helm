//! 应用层：常驻服务编排（创建 / 列表 / 启停 / 重启）。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::store::Db;
use crate::store::agent_repo::AgentRepo;
use crate::store::service_repo::{ServiceRepo, ServiceRow};
use helm_proto::pb::{ServerMessage, ServiceStart, ServiceStop, server_message};
use uuid::Uuid;

/// 常驻服务用例。
#[derive(Clone)]
pub struct ServiceService {
    db: Db,
    registry: ConnectionRegistry,
}

impl ServiceService {
    pub fn new(db: Db, registry: ConnectionRegistry) -> Self {
        Self { db, registry }
    }

    /// 创建服务（绑定到 agent 所在的 host）。
    pub async fn create(
        &self,
        agent_id: &str,
        name: &str,
        command: &str,
        args: &[String],
        restart_policy: &str,
    ) -> Result<ServiceRow> {
        let host_id = AgentRepo::new(self.db.clone())
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;
        let rp = if restart_policy == "always" {
            "always"
        } else {
            "no"
        };
        Ok(ServiceRepo::new(self.db.clone())
            .create(host_id, name, command, args, rp)
            .await?)
    }

    /// 列出所有服务。
    pub async fn list(&self) -> Result<Vec<ServiceRow>> {
        Ok(ServiceRepo::new(self.db.clone()).list().await?)
    }

    /// 启动服务：找到 host 的在线 agent，下发 ServiceStart。
    pub async fn start(&self, id: Uuid) -> Result<()> {
        let svc = ServiceRepo::new(self.db.clone())
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("service: {id}")))?;
        let agent_id = self
            .online_agent(svc.host_id)
            .await
            .ok_or_else(|| Error::NotConnected(svc.host_id.to_string()))?;

        let msg = ServerMessage {
            kind: Some(server_message::Kind::ServiceStart(ServiceStart {
                service_id: id.to_string(),
                command: svc.command.clone(),
                args: svc.args.clone(),
                restart_policy: svc.restart_policy.clone(),
            })),
        };
        self.registry
            .send(&agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;
        ServiceRepo::new(self.db.clone())
            .set_status(id, "running", None, None)
            .await?;
        Ok(())
    }

    /// 停止服务：下发 ServiceStop。
    pub async fn stop(&self, id: Uuid) -> Result<()> {
        let svc = ServiceRepo::new(self.db.clone())
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("service: {id}")))?;
        let agent_id = self
            .online_agent(svc.host_id)
            .await
            .ok_or_else(|| Error::NotConnected(svc.host_id.to_string()))?;

        let msg = ServerMessage {
            kind: Some(server_message::Kind::ServiceStop(ServiceStop {
                service_id: id.to_string(),
            })),
        };
        self.registry
            .send(&agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;
        ServiceRepo::new(self.db.clone())
            .set_status(id, "stopped", None, None)
            .await?;
        Ok(())
    }

    /// 重启服务：下发 ServiceStart（agent 端先停旧进程再启）。
    pub async fn restart(&self, id: Uuid) -> Result<()> {
        self.start(id).await
    }

    /// 查单个服务。
    pub async fn get(&self, id: Uuid) -> Result<ServiceRow> {
        ServiceRepo::new(self.db.clone())
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("service: {id}")))
    }

    /// 找到 host 的一个在线 agent。
    async fn online_agent(&self, host_id: Uuid) -> Option<String> {
        let ids = AgentRepo::new(self.db.clone())
            .list_agent_ids(host_id)
            .await
            .ok()?;
        for id in ids {
            if self.registry.is_online(&id).await {
                return Some(id);
            }
        }
        None
    }
}
