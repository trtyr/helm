//! 注册流程的各阶段（G7 拆分，2026-09-20）：落库 → 登记连接 → 补执行 → 上线事件 → 入站泵。
//!
//! 这些方法都是 [`AgentServiceImpl`] 的阶段实现，由父模块的 `open_channel` 按序调用；
//! 拆出来只为让「服务入口 + JSON/gRPC 信封」与「一条连接的注册生命周期」各自可读。
//! 方法对本模块 **pub(super)** ——父模块需要调用它们。
//!
//! [`AgentServiceImpl`]: super::AgentServiceImpl

use super::AgentServiceImpl;
use crate::application::agent_lifecycle_service::AgentLifecycleService;
use crate::application::exec_service::ExecService;
use crate::grpc::connection_registry::RegisteredConnection;
use crate::grpc::inbound::{InboundCtx, InboundCtxDeps};
use helm_proto::pb::{
    AgentMessage, JobCancel, Register, SelfDestruct, ServerMessage, server_message,
};
use tokio::sync::mpsc;
use tonic::{Status, Streaming};
use uuid::Uuid;

impl AgentServiceImpl {
    /// 阶段 2：落库 host + agent（失败不阻断连接，仅告警）。
    ///
    /// G3 收口（2026-09-21）：落库经 `AgentLifecycleService`，grpc 层不再直构 `AgentRepo`。
    pub(super) async fn persist_agent(
        &self,
        agent_id: &str,
        register: &Register,
        public_ip: &str,
    ) -> Option<Uuid> {
        match AgentLifecycleService::new(self.db.clone(), self.registry.clone())
            .register_agent(agent_id, register, public_ip)
            .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!(agent_id = %agent_id, error = %e, "failed to persist agent");
                None
            }
        }
    }

    /// 阶段 3：登记连接。容量满即拒绝（E3，agent 侧按重连退避重试）；
    /// 同 id 重复注册则顶掉旧连接。
    pub(super) async fn register_connection(
        &self,
        agent_id: &str,
        register: &Register,
        tx: mpsc::Sender<ServerMessage>,
    ) -> Result<RegisteredConnection, Status> {
        let registration = match self.registry.register(agent_id, tx).await {
            Ok(reg) => reg,
            Err(e) => {
                // E3：注册表容量已满——拒绝连接（不广播上线）
                tracing::error!(agent_id = %agent_id, error = %e, "registration rejected: registry full");
                return Err(Status::resource_exhausted(e.to_string()));
            }
        };
        if let Some(old) = registration.replaced.as_ref() {
            tracing::info!(agent_id = %agent_id, "duplicate registration, kicking previous connection");
            old.kick();
        }
        tracing::info!(
            agent_id = %agent_id,
            version = %register.version,
            hostname = %register.host.as_ref().map(|h| h.hostname.as_str()).unwrap_or(""),
            "agent registered"
        );
        Ok(registration)
    }

    /// 阶段 5a：补执行掉线期间挂起的卸载/注销（否则进程残留 = "下线了但还在"）。
    pub(super) async fn run_deferred_offline(
        &self,
        agent_id: &str,
        tx: &mpsc::Sender<ServerMessage>,
    ) -> bool {
        let pending = AgentLifecycleService::new(self.db.clone(), self.registry.clone())
            .take_pending_offline(agent_id)
            .await
            .ok()
            .flatten();
        let Some(action) = pending else {
            return false;
        };
        tracing::info!(agent_id = %agent_id, %action, "deferred offline command executed on reconnect");
        let remove_binary = action == "uninstall";
        // 对端已断则自毁指令无处送达（下方仍会删档，两种失败都会在日志/对账里体现）
        let _ = tx
            .send(ServerMessage {
                kind: Some(server_message::Kind::SelfDestruct(SelfDestruct {
                    remove_binary,
                })),
            })
            .await;
        // ⚠ 顺序风险：上面若发送失败，而下面仍删档，agent 就变成「还在跑但 server 无档案」。
        // 发送结果必须可见，便于事后对账（两种失败的组合会在日志里同时出现）。
        if let Err(e) = AgentLifecycleService::new(self.db.clone(), self.registry.clone())
            .remove_record(agent_id)
            .await
        {
            tracing::warn!(agent_id = %agent_id, error = %e, "failed to delete agent after deferred uninstall");
        }
        true
    }

    /// 阶段 5b：补发掉线期间挂起的 job 取消（EN-64 ③），杀掉目标机残留进程。
    ///
    /// job 在 cancel 时已置 `cancelled` 终态，此处发送只为杀进程；agent 回报的
    /// `ExecResult(cancelled=true)` 幂等覆盖，不改变终态。
    pub(super) async fn run_deferred_cancels(
        &self,
        agent_id: &str,
        tx: &mpsc::Sender<ServerMessage>,
    ) {
        match ExecService::new(self.db.clone(), self.registry.clone())
            .take_pending_cancels(agent_id)
            .await
        {
            Ok(job_ids) if !job_ids.is_empty() => {
                tracing::info!(
                    agent_id = %agent_id,
                    count = job_ids.len(),
                    "deferred job cancels executed on reconnect"
                );
                for jid in job_ids {
                    // 补发取消只为杀进程：agent 可能再次掉线，失败时进程残留但 job 已是
                    // cancelled 终态；失败留痕以便运维排查残留进程
                    if let Err(e) = tx
                        .send(ServerMessage {
                            kind: Some(server_message::Kind::JobCancel(JobCancel {
                                job_id: jid.to_string(),
                            })),
                        })
                        .await
                    {
                        tracing::warn!(agent_id = %agent_id, job_id = %jid, error = %e, "deferred job cancel not delivered");
                    }
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(agent_id = %agent_id, error = %e, "take_pending_cancels failed")
            }
        }
    }

    /// 阶段 6：上线事件（决策 009 / P003 T1）。
    ///
    /// 落库失败（无 host_id）、挂起补执行、重复注册（本就在线，仅换连接）三种情况不通知。
    pub(super) async fn notify_online(
        &self,
        host_id: Option<Uuid>,
        register: &Register,
        deferred_offline: bool,
        replaced: bool,
    ) {
        if let Some(hid) = host_id
            && !deferred_offline
            && !replaced
        {
            let hostname = register
                .host
                .as_ref()
                .map(|h| h.hostname.as_str())
                .unwrap_or("");
            // P003 T1：上线落库 + 通知——reverse/forward 共用的唯一实现
            crate::application::notification_service::record_online_and_notify(
                &self.db,
                &self.streams,
                hid,
                hostname,
            )
            .await;
        }
    }

    /// 阶段 7：后台消费入站流；流结束时注销连接并落离线事件。
    pub(super) fn spawn_inbound_pump(
        &self,
        inbound: Streaming<AgentMessage>,
        registration: RegisteredConnection,
        host_id: Option<Uuid>,
        hostname: String,
        agent_id: String,
    ) {
        let deps = InboundCtxDeps {
            agent_id: agent_id.clone(),
            host_id,
            hostname,
            registry: self.registry.clone(),
            kick_tx: registration.kick_tx,
            transfers: self.transfers.clone(),
            sessions: self.sessions.clone(),
            file_list: self.file_list.clone(),
            query: self.query.clone(),
            streams: self.streams.clone(),
            metrics: self.metrics.clone(),
            db: self.db.clone(),
        };
        let mut kick_rx = registration.kick_rx;
        tokio::spawn(async move {
            let mut ctx = InboundCtx::new(deps);
            let mut inbound = inbound;
            let mut disc_reason = "stream_closed";
            let mut disc_detail = String::new();
            loop {
                tokio::select! {
                    // 被同 id 新注册顶掉：立即退出并释放流（不入流则旧 HTTP/2 流复位不了，TCP 泄漏）
                    // （wait_for 的 watch::Ref 非 Send，包一层 async 块在内部丢弃）
                    _ = async {
                        // watch 通道：发送端 drop（连接对象已释放）时 wait_for 返回 Err，
                        // 语义等价于「不会再有 kick」，退出循环正确
                        let _ = kick_rx.wait_for(|kicked| *kicked).await;
                    } => break,
                    msg = inbound.message() => match msg {
                        Ok(Some(msg)) => ctx.handle(msg).await,
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(agent_id = %agent_id, error = %e, "inbound stream error");
                            disc_reason = "transport_error";
                            disc_detail = e.to_string();
                            break;
                        }
                    }
                }
            }
            ctx.on_disconnect(disc_reason, &disc_detail).await;
        });
    }
}
