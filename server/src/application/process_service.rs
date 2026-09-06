//! 应用层：进程管理 + 网络信息采集。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::query_registry::{QueryRegistry, QueryResponse};
use helm_proto::pb::{
    NetInfo, NetInfoResult, ProcessInfo, ProcessKill, ProcessList, ServerMessage, SysServiceAction,
    SysServiceList, SysServiceListResult, server_message,
};
use uuid::Uuid;

const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 进程/网络查询用例。
#[derive(Clone)]
pub struct ProcessService {
    registry: ConnectionRegistry,
    query: QueryRegistry,
}

impl ProcessService {
    pub fn new(registry: ConnectionRegistry, query: QueryRegistry) -> Self {
        Self { registry, query }
    }

    /// 下发一次查询并等待结果。
    async fn request(
        &self,
        agent_id: &str,
        request_id: &str,
        kind: server_message::Kind,
    ) -> Result<QueryResponse> {
        let rx = self.query.register(request_id.to_string()).await;
        let msg = ServerMessage { kind: Some(kind) };
        self.registry
            .send(agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;
        tokio::time::timeout(QUERY_TIMEOUT, rx)
            .await
            .map_err(|_| Error::Internal("query timeout".into()))?
            .map_err(|_| Error::Internal("query channel closed".into()))
    }

    /// 列出进程。
    pub async fn list(&self, agent_id: &str) -> Result<Vec<ProcessInfo>> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::ProcessList(ProcessList {
                    request_id: request_id.clone(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::ProcessList(r) => Ok(r.processes),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 终止进程。
    pub async fn kill(&self, agent_id: &str, pid: i32) -> Result<bool> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::ProcessKill(ProcessKill {
                    request_id: request_id.clone(),
                    pid,
                }),
            )
            .await?;
        match resp {
            QueryResponse::ProcessKill(r) => Ok(r.ok),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 采集网络信息。
    pub async fn net(&self, agent_id: &str) -> Result<NetInfoResult> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::NetInfo(NetInfo {
                    request_id: request_id.clone(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::NetInfo(r) => Ok(r),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 枚举目标机系统服务。
    pub async fn sys_services(&self, agent_id: &str) -> Result<SysServiceListResult> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::SysServiceList(SysServiceList {
                    request_id: request_id.clone(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::SysServiceList(r) => Ok(r),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 系统服务操作（start / stop / restart）。
    pub async fn sys_service_action(
        &self,
        agent_id: &str,
        name: &str,
        action: &str,
    ) -> Result<(bool, Option<String>)> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::SysServiceAction(SysServiceAction {
                    request_id: request_id.clone(),
                    name: name.to_string(),
                    action: action.to_string(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::SysServiceAction(r) => Ok((r.ok, r.error)),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }
}
