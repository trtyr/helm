//! AgentService 实现：处理 Agent 反向连入的双向流。

use std::pin::Pin;

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::inbound::InboundCtx;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::agent_repo::HostOsDetails;
use crate::store::{Db, agent_repo::AgentRepo, job_repo::JobRepo};
use helm_proto::pb::{
    AgentMessage, JobCancel, RegisterAck, SelfDestruct, ServerMessage, agent_message,
    agent_service_server::AgentService, server_message,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::async_trait;
use tonic::{Request, Response, Status, Streaming};

/// AgentService 实现：持有连接注册表、数据库与认证 token。
pub struct AgentServiceImpl {
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
    sessions: SessionRegistry,
    file_list: FileListRegistry,
    query: QueryRegistry,
    streams: StreamRegistry,
    /// 指标落库队列（E2）。
    metrics: crate::application::metric_sink::MetricSink,
    db: Db,
    server_token: String,
}

impl AgentServiceImpl {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        registry: ConnectionRegistry,
        transfers: TransferRegistry,
        sessions: SessionRegistry,
        file_list: FileListRegistry,
        query: QueryRegistry,
        streams: StreamRegistry,
        metrics: crate::application::metric_sink::MetricSink,
        db: Db,
        server_token: String,
    ) -> Self {
        Self {
            registry,
            transfers,
            sessions,
            file_list,
            query,
            streams,
            metrics,
            db,
            server_token,
        }
    }

    /// 校验注册 token；严格匹配，不匹配即拒绝。
    fn verify_token(&self, token: &str) -> bool {
        token_matches(&self.server_token, token)
    }
}

#[async_trait]
impl AgentService for AgentServiceImpl {
    type OpenChannelStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<ServerMessage, Status>> + Send>>;

    async fn open_channel(
        &self,
        request: Request<Streaming<AgentMessage>>,
    ) -> Result<Response<Self::OpenChannelStream>, Status> {
        // 外网 IP：Server 看到的连接源地址（NAT 后即出口公网地址）；into_inner 会拿走 request，须先取
        let public_ip = request
            .remote_addr()
            .map(|a| a.ip().to_string())
            .unwrap_or_default();
        let mut inbound = request.into_inner();

        // 首条消息必须是 Register
        let first = inbound
            .message()
            .await?
            .ok_or_else(|| Status::invalid_argument("channel closed before Register"))?;
        let register = match first.kind {
            Some(agent_message::Kind::Register(r)) => r,
            _ => return Err(Status::invalid_argument("first message must be Register")),
        };

        if !self.verify_token(&register.token) {
            return Err(Status::unauthenticated("invalid token"));
        }

        let agent_id = register.agent_id.clone();

        // 落库 host + agent（失败不阻断连接，仅告警）
        let hostname = register
            .host
            .as_ref()
            .map(|h| h.hostname.as_str())
            .unwrap_or("");
        let os = register.host.as_ref().map(|h| h.os.as_str()).unwrap_or("");
        let arch = register
            .host
            .as_ref()
            .map(|h| h.arch.as_str())
            .unwrap_or("");
        let platform = register
            .host
            .as_ref()
            .map(|h| h.platform.as_str())
            .unwrap_or("");
        let local_ips = register
            .host
            .as_ref()
            .map(|h| h.local_ips.clone())
            .unwrap_or_default();
        let elevated = register.host.as_ref().map(|h| h.elevated).unwrap_or(false);
        let details = register
            .host
            .as_ref()
            .map(|h| HostOsDetails {
                os_version: &h.os_version,
                kernel: &h.kernel,
                uptime_secs: h.uptime_secs,
            })
            .unwrap_or_default();
        let host_id = match AgentRepo::new(self.db.clone())
            .register(
                &agent_id,
                &register.version,
                hostname,
                os,
                arch,
                platform,
                &public_ip,
                &local_ips,
                elevated,
                details,
            )
            .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!(agent_id = %agent_id, error = %e, "failed to persist agent");
                None
            }
        };

        let (tx, rx) = mpsc::channel::<ServerMessage>(64);
        let registration = match self.registry.register(&agent_id, tx.clone()).await {
            Ok(reg) => reg,
            Err(e) => {
                // E3：注册表容量已满——拒绝连接（不广播上线），
                // agent 侧收到连接错误后按重连退避重试
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
            hostname = %hostname,
            "agent registered"
        );

        // 回 RegisterAck
        let ack = ServerMessage {
            kind: Some(server_message::Kind::RegisterAck(RegisterAck {
                ok: true,
                message: "registered".into(),
                heartbeat_interval_secs: 10,
            })),
        };
        let _ = tx.send(ack).await;

        // 掉线期间挂起的卸载/注销：重连瞬间补执行（否则进程残留 = "下线了但还在"）
        let pending = AgentRepo::new(self.db.clone())
            .take_pending_offline(&agent_id)
            .await
            .unwrap_or(None);
        let mut deferred_offline = false;
        if let Some(action) = pending {
            deferred_offline = true;
            tracing::info!(agent_id = %agent_id, %action, "deferred offline command executed on reconnect");
            let remove_binary = action == "uninstall";
            let _ = tx
                .send(ServerMessage {
                    kind: Some(server_message::Kind::SelfDestruct(SelfDestruct {
                        remove_binary,
                    })),
                })
                .await;
            let _ = AgentRepo::new(self.db.clone()).delete(&agent_id).await;
        }

        // 掉线期间挂起的 job 取消（EN-64 ③）：重连后补发 JobCancel，杀掉目标机残留进程。
        // job 在 cancel 时已置 cancelled 终态，此处发送只为杀进程；agent 回报的
        // ExecResult(cancelled=true) 幂等覆盖，不改变终态。
        match JobRepo::new(self.db.clone())
            .take_pending_cancels(&agent_id)
            .await
        {
            Ok(job_ids) if !job_ids.is_empty() => {
                tracing::info!(
                    agent_id = %agent_id,
                    count = job_ids.len(),
                    "deferred job cancels executed on reconnect"
                );
                for jid in job_ids {
                    let _ = tx
                        .send(ServerMessage {
                            kind: Some(server_message::Kind::JobCancel(JobCancel {
                                job_id: jid.to_string(),
                            })),
                        })
                        .await;
                }
            }
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(agent_id = %agent_id, error = %e, "take_pending_cancels failed")
            }
        }

        // 上线通知（决策 009：系统内小卡片；落库失败无 host_id 则跳过；挂起补执行或
        // 重复注册（本就在线，仅换连接）时不通知）
        if let Some(hid) = host_id
            && !deferred_offline
            && registration.replaced.is_none()
        {
            let svc = crate::application::notification_service::NotificationService::new(
                self.db.clone(),
                self.streams.clone(),
            );
            if let Err(e) = svc
                .notify(
                    hid,
                    crate::application::notification_service::KIND_ONLINE,
                    &format!("主机 {hostname} 已上线"),
                )
                .await
            {
                tracing::warn!(agent_id = %agent_id, error = ?e, "online notify failed");
            }
        }

        // 后台任务：消费入站流；流结束时注销。
        let registry = self.registry.clone();
        let transfers = self.transfers.clone();
        let sessions = self.sessions.clone();
        let file_list = self.file_list.clone();
        let query = self.query.clone();
        let streams = self.streams.clone();
        let metrics = self.metrics.clone();
        let db = self.db.clone();
        let agent_id_inner = agent_id.clone();
        let hostname_inner = hostname.to_string();
        tokio::spawn(async move {
            let mut ctx = InboundCtx::new(
                agent_id_inner.clone(),
                host_id,
                hostname_inner,
                registry,
                registration.kick_tx,
                transfers,
                sessions,
                file_list,
                query,
                streams,
                metrics,
                db,
            );
            let mut kick_rx = registration.kick_rx;
            loop {
                tokio::select! {
                    // 被同 id 新注册顶掉：立即退出并释放流（不入流则旧 HTTP/2 流复位不了，TCP 泄漏）
                    // （wait_for 的 watch::Ref 非 Send，包一层 async 块在内部丢弃）
                    _ = async {
                        let _ = kick_rx.wait_for(|kicked| *kicked).await;
                    } => break,
                    msg = inbound.message() => match msg {
                        Ok(Some(msg)) => ctx.handle(msg).await,
                        Ok(None) => break,
                        Err(e) => {
                            tracing::warn!(agent_id = %agent_id_inner, error = %e, "inbound stream error");
                            break;
                        }
                    }
                }
            }
            ctx.on_disconnect().await;
        });

        let outbound = ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(outbound)))
    }
}

/// 校验注册 token：严格匹配，且 server_token 为空时拒绝所有（纯函数，便于测试）。
pub fn token_matches(server_token: &str, provided: &str) -> bool {
    !server_token.is_empty() && provided == server_token
}

/// 由执行结果判断 Job 终态（纯函数，便于测试）。
/// `cancelled`/`timed_out` 由 agent 显式上报（JobCancel 杀进程 / timeout_secs 超时），
/// 优先于 error/exit_code 判定（EN-64）。
pub fn job_status(
    error: Option<&str>,
    exit_code: Option<i32>,
    cancelled: bool,
    timed_out: bool,
) -> &'static str {
    if cancelled {
        "cancelled"
    } else if timed_out {
        "timed_out"
    } else if error.is_some() {
        "failed"
    } else {
        match exit_code {
            Some(0) => "succeeded",
            Some(_) => "failed",
            None => "succeeded",
        }
    }
}

/// 将 agent 上报的服务状态映射为 DB status（纯函数，便于测试）。
pub fn map_service_status(status: &str) -> Option<&'static str> {
    match status {
        "running" => Some("running"),
        "failed" => Some("failed"),
        "exited" => Some("stopped"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_strict() {
        assert!(token_matches("secret", "secret"));
        assert!(!token_matches("secret", "wrong"));
        assert!(!token_matches("secret", ""));
        // 空 server_token 也拒绝（默认值非空，严格匹配）
        assert!(!token_matches("", ""));
    }

    #[test]
    fn job_status_rules() {
        assert_eq!(job_status(None, Some(0), false, false), "succeeded");
        assert_eq!(job_status(None, Some(1), false, false), "failed");
        assert_eq!(job_status(Some("boom"), None, false, false), "failed");
        assert_eq!(job_status(None, None, false, false), "succeeded");
        // EN-64：取消与超时标志优先于 error/exit_code
        assert_eq!(
            job_status(Some("killed"), Some(-9), true, false),
            "cancelled"
        );
        assert_eq!(job_status(Some("timeout"), None, false, true), "timed_out");
    }

    #[test]
    fn map_service_status_rules() {
        assert_eq!(map_service_status("running"), Some("running"));
        assert_eq!(map_service_status("failed"), Some("failed"));
        assert_eq!(map_service_status("exited"), Some("stopped"));
        assert_eq!(map_service_status("unknown"), None);
    }
}
