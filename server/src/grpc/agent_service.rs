//! AgentService 实现：处理 Agent 反向连入的双向流。

use std::pin::Pin;

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::inbound::InboundCtx;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::{Db, agent_repo::AgentRepo};
use helm_proto::pb::{
    AgentMessage, RegisterAck, ServerMessage, agent_message, agent_service_server::AgentService,
    server_message,
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
        let host_id = match AgentRepo::new(self.db.clone())
            .register(&agent_id, &register.version, hostname, os, arch, platform)
            .await
        {
            Ok(id) => Some(id),
            Err(e) => {
                tracing::warn!(agent_id = %agent_id, error = %e, "failed to persist agent");
                None
            }
        };

        let (tx, rx) = mpsc::channel::<ServerMessage>(64);
        self.registry.register(&agent_id, tx.clone()).await;
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

        // 上线通知（决策 009：系统内小卡片；落库失败无 host_id 则跳过）
        if let Some(hid) = host_id {
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
        let db = self.db.clone();
        let agent_id_inner = agent_id.clone();
        let hostname_inner = hostname.to_string();
        tokio::spawn(async move {
            let mut ctx = InboundCtx::new(
                agent_id_inner.clone(),
                host_id,
                hostname_inner,
                registry,
                transfers,
                sessions,
                file_list,
                query,
                streams,
                db,
            );
            loop {
                match inbound.message().await {
                    Ok(Some(msg)) => ctx.handle(msg).await,
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(agent_id = %agent_id_inner, error = %e, "inbound stream error");
                        break;
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
pub fn job_status(error: Option<&str>, exit_code: Option<i32>) -> &'static str {
    if error.is_some() {
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
        assert_eq!(job_status(None, Some(0)), "succeeded");
        assert_eq!(job_status(None, Some(1)), "failed");
        assert_eq!(job_status(Some("boom"), None), "failed");
        assert_eq!(job_status(None, None), "succeeded");
    }

    #[test]
    fn map_service_status_rules() {
        assert_eq!(map_service_status("running"), Some("running"));
        assert_eq!(map_service_status("failed"), Some("failed"));
        assert_eq!(map_service_status("exited"), Some("stopped"));
        assert_eq!(map_service_status("unknown"), None);
    }
}
