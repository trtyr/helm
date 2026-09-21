//! 应用层：命令执行编排。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::store::{Db, agent_repo::AgentRepo, job_repo::JobRepo};
use helm_proto::pb::{ExecRequest, JobCancel, ServerMessage, server_message};
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
    /// `timeout_secs` 透传给 agent（由 agent 侧超时杀进程并以 timed_out 上报）；
    /// None = agent 不限时，仅由 Server 侧 job sweeper 兜底。
    pub async fn exec(
        &self,
        agent_id: &str,
        command: &str,
        args: &[String],
        timeout_secs: Option<u32>,
    ) -> Result<Uuid> {
        let host_id = AgentRepo::new(self.db.clone())
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;

        let job = JobRepo::new(self.db.clone())
            .create(host_id, command, args)
            .await?;

        let req = ServerMessage {
            kind: Some(server_message::Kind::ExecRequest(ExecRequest {
                job_id: job.id.to_string(),
                command: command.to_string(),
                args: args.to_vec(),
                timeout_secs,
                working_dir: None,
            })),
        };

        self.registry
            .send(agent_id, req)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;
        JobRepo::new(self.db.clone())
            .set_status(job.id, "running")
            .await?;

        Ok(job.id)
    }

    /// 取消结果：
    /// `delivered` = JobCancel 已送达在线 agent（真中断）；
    /// `compensated` = agent 离线，已记补偿（重连后补发 JobCancel 杀目标机残留进程，EN-64 ③）。
    pub async fn cancel(&self, job_id: Uuid) -> Result<(bool, bool, bool)> {
        let job = JobRepo::new(self.db.clone())
            .get(job_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("job: {job_id}")))?;
        if job.status != "queued" && job.status != "running" {
            return Err(Error::InvalidArgument(format!(
                "job already finished: {}",
                job.status
            )));
        }

        // jobs 表只有 host_id，经最近注册的 agent 找连接
        let agent_id = AgentRepo::new(self.db.clone())
            .find_latest_agent_id(job.host_id)
            .await?;

        let mut delivered = false;
        let mut compensated = false;
        if job.status == "running"
            && let Some(agent_id) = agent_id.as_deref()
        {
            if self.registry.is_online(agent_id).await {
                let msg = ServerMessage {
                    kind: Some(server_message::Kind::JobCancel(JobCancel {
                        job_id: job_id.to_string(),
                    })),
                };
                self.registry
                    .send(agent_id, msg)
                    .await
                    .map_err(|e| Error::NotConnected(e.to_string()))?;
                delivered = true;
            } else {
                // 离线：目标机上进程仍在跑，记补偿，重连后补杀
                JobRepo::new(self.db.clone())
                    .mark_cancel_pending(agent_id, job_id)
                    .await?;
                compensated = true;
            }
        }

        // queued：从未送达，直接收敛；running：终态先行（agent 回报 ExecResult(cancelled=true) 时幂等覆盖）
        JobRepo::new(self.db.clone())
            .set_status(job_id, "cancelled")
            .await?;
        Ok((true, delivered, compensated))
    }

    // ---- G3 收口（2026-09-21）：grpc 层不再直构 store 仓储 ----

    /// Agent 回报的执行终态落库；返回 `false` 表示被幂等守卫拦下（重放/迟到回报），非错误。
    pub async fn finish_job(
        &self,
        job_id: Uuid,
        status: &str,
        output: &str,
        exit_code: Option<i32>,
    ) -> Result<bool> {
        let written = JobRepo::new(self.db.clone())
            .finish(job_id, status, output, exit_code)
            .await?;
        Ok(written)
    }

    /// 取出掉线期间挂起的取消请求（EN-64 ③），重连时补杀目标机残留进程。
    pub async fn take_pending_cancels(&self, agent_id: &str) -> Result<Vec<Uuid>> {
        let ids = JobRepo::new(self.db.clone())
            .take_pending_cancels(agent_id)
            .await?;
        Ok(ids)
    }
}
