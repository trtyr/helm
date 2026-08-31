//! 应用层：命令执行编排。

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::store::{Db, agent_repo::AgentRepo, job_repo::JobRepo};
use anyhow::anyhow;
use helm_proto::pb::{ExecRequest, ServerMessage, server_message};
use uuid::Uuid;

/// 命令执行用例：创建 Job → 经连接下发 → 状态流转。
#[derive(Clone)]
pub struct ExecService {
    db: Db,
    registry: ConnectionRegistry,
}

impl ExecService {
    pub fn new(db: Db, registry: ConnectionRegistry) -> Self {
        Self { db, registry }
    }

    /// 向指定 Agent 下发命令，返回 job_id。
    pub async fn exec(
        &self,
        agent_id: &str,
        command: &str,
        args: &[String],
    ) -> anyhow::Result<Uuid> {
        let host_id = AgentRepo::new(self.db.clone())
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| anyhow!("unknown agent: {agent_id}"))?;

        let job = JobRepo::new(self.db.clone())
            .create(host_id, command, args)
            .await?;

        let req = ServerMessage {
            kind: Some(server_message::Kind::ExecRequest(ExecRequest {
                job_id: job.id.to_string(),
                command: command.to_string(),
                args: args.to_vec(),
                timeout_secs: None,
                working_dir: None,
            })),
        };

        self.registry.send(agent_id, req).await?;
        JobRepo::new(self.db.clone())
            .set_status(job.id, "running")
            .await?;

        Ok(job.id)
    }
}
