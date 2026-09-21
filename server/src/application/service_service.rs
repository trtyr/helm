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

    /// 分页列出服务。
    pub async fn list_paged(&self, limit: i64, offset: i64) -> Result<Vec<ServiceRow>> {
        Ok(ServiceRepo::new(self.db.clone())
            .list_paged(limit, offset)
            .await?)
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

    /// 更新服务（name/command/args/restart_policy）。
    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        command: &str,
        args: &[String],
        restart_policy: &str,
    ) -> Result<ServiceRow> {
        let rp = if restart_policy == "always" {
            "always"
        } else {
            "no"
        };
        ServiceRepo::new(self.db.clone())
            .update(id, name, command, args, rp)
            .await?
            .ok_or_else(|| Error::NotFound(format!("service: {id}")))
    }

    /// 删除服务（运行中先停）。
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        let svc = self.get(id).await?;
        if svc.status == "running"
            && let Err(e) = self.stop(id).await
        {
            // 停不掉也要继续删记录，但必须留痕（否则进程可能仍在跑）
            tracing::warn!(service_id = %id, error = %e, "failed to stop service before delete");
        }
        let n = ServiceRepo::new(self.db.clone()).delete(id).await?;
        if n == 0 {
            return Err(Error::NotFound(format!("service: {id}")));
        }
        Ok(())
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

    // ---- G3 收口（2026-09-21）：grpc 层不再直构 store 仓储 ----

    /// Agent 上报的服务日志追加（历史可读性用；失败非致命，由调用方决定是否告警）。
    pub async fn append_log(&self, id: Uuid, log: &[u8]) -> Result<()> {
        ServiceRepo::new(self.db.clone())
            .append_log(id, log)
            .await?;
        Ok(())
    }

    /// Agent 上报的服务状态落库。
    pub async fn report_status(
        &self,
        id: Uuid,
        status: &str,
        pid: Option<i32>,
        exit_code: Option<i32>,
    ) -> Result<()> {
        ServiceRepo::new(self.db.clone())
            .set_status(id, status, pid, exit_code)
            .await?;
        Ok(())
    }
}
