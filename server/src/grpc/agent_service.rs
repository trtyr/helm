//! AgentService 实现：处理 Agent 反向连入的双向流。

use std::collections::HashMap;
use std::pin::Pin;

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::query_registry::{QueryRegistry, QueryResponse};
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::alert_repo::AlertRepo;
use crate::store::service_repo::ServiceRepo;
use crate::store::{Db, agent_repo::AgentRepo, job_repo::JobRepo, metric_repo::MetricRepo};
use helm_proto::pb::{
    AgentMessage, RegisterAck, ServerMessage, agent_message, agent_service_server::AgentService,
    server_message,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::async_trait;
use tonic::{Request, Response, Status, Streaming};
use uuid::Uuid;

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

        // 后台任务：消费入站流；流结束时注销。
        let registry = self.registry.clone();
        let transfers = self.transfers.clone();
        let sessions = self.sessions.clone();
        let file_list = self.file_list.clone();
        let query = self.query.clone();
        let streams = self.streams.clone();
        let db = self.db.clone();
        let agent_id_inner = agent_id.clone();
        tokio::spawn(async move {
            let job_repo = JobRepo::new(db.clone());
            let metric_repo = MetricRepo::new(db.clone());
            let agent_repo = AgentRepo::new(db.clone());
            let alert_repo = AlertRepo::new(db.clone());
            let mut outputs: HashMap<String, String> = HashMap::new();

            loop {
                match inbound.message().await {
                    Ok(Some(msg)) => match msg.kind {
                        Some(agent_message::Kind::Heartbeat(h)) => {
                            if let Err(e) = agent_repo
                                .update_heartbeat(&agent_id_inner, h.timestamp_unix_ms)
                                .await
                            {
                                tracing::warn!(
                                    agent_id = %agent_id_inner,
                                    error = %e,
                                    "failed to update heartbeat"
                                );
                            }
                            tracing::debug!(
                                agent_id = %agent_id_inner,
                                ts_ms = h.timestamp_unix_ms,
                                "heartbeat"
                            );
                        }
                        Some(agent_message::Kind::MetricReport(report)) => {
                            let count = report.metrics.len();
                            if let Some(host_id) = host_id {
                                for m in report.metrics {
                                    let ts =
                                        chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                                            m.timestamp_unix_ms as i64,
                                        )
                                        .unwrap_or_else(chrono::Utc::now);
                                    if let Err(e) =
                                        metric_repo.insert(host_id, &m.name, m.value, ts).await
                                    {
                                        tracing::warn!(error = %e, "failed to persist metric");
                                    }
                                    // 告警评估：超阈值落 alerts 表
                                    if let Some(threshold) =
                                        crate::application::alert_service::AlertService::threshold_for(&m.name)
                                            .filter(|t| m.value > *t)
                                    {
                                        let _ = alert_repo
                                            .insert(host_id, &m.name, threshold, m.value)
                                            .await;
                                    }
                                    // 实时流：推送指标
                                    let payload = serde_json::json!({
                                        "host_id": host_id,
                                        "name": m.name,
                                        "value": m.value,
                                        "ts": m.timestamp_unix_ms,
                                    });
                                    streams
                                        .broadcast("metrics", payload.to_string().into_bytes())
                                        .await;
                                }
                            }
                            tracing::debug!(agent_id = %agent_id_inner, count, "metrics received");
                        }
                        Some(agent_message::Kind::ExecResult(er)) => {
                            let job_id = er.job_id.clone();
                            let entry = outputs.entry(job_id.clone()).or_default();
                            if let Some(chunk) = er.chunk {
                                entry.push_str(&String::from_utf8_lossy(&chunk.data));
                                // 实时流：推送 job 输出增量
                                streams
                                    .broadcast(&format!("job:{job_id}"), chunk.data)
                                    .await;
                            }
                            if er.finished {
                                let output = outputs.remove(&job_id).unwrap_or_default();
                                let status = job_status(er.error.as_deref(), er.exit_code);
                                match Uuid::parse_str(&job_id) {
                                    Ok(id) => {
                                        if let Err(e) =
                                            job_repo.finish(id, status, &output, er.exit_code).await
                                        {
                                            tracing::warn!(
                                                job_id = %job_id,
                                                error = %e,
                                                "failed to persist job result"
                                            );
                                        } else {
                                            tracing::info!(job_id = %job_id, status, "job finished");
                                        }
                                    }
                                    Err(_) => tracing::warn!(job_id = %job_id, "invalid job_id"),
                                }
                            }
                        }
                        Some(agent_message::Kind::FileChunk(chunk)) => {
                            transfers
                                .accumulate_chunk(&chunk.transfer_id, &chunk.data)
                                .await;
                        }
                        Some(agent_message::Kind::FileStatus(status)) => {
                            let tid = status.transfer_id.clone();
                            transfers.complete(&tid, status).await;
                        }
                        Some(agent_message::Kind::SessionOpened(opened)) => {
                            tracing::info!(session_id = %opened.session_id, "session opened");
                        }
                        Some(agent_message::Kind::SessionOutput(out)) => {
                            let _ = sessions.forward(&out.session_id, out.data).await;
                        }
                        Some(agent_message::Kind::SessionClosed(closed)) => {
                            sessions.unregister(&closed.session_id).await;
                            tracing::info!(session_id = %closed.session_id, "session closed");
                        }
                        Some(agent_message::Kind::ServiceStatus(st)) => {
                            if let Ok(id) = Uuid::parse_str(&st.service_id) {
                                let repo = ServiceRepo::new(db.clone());
                                if !st.log.is_empty() {
                                    let _ = repo.append_log(id, &st.log).await;
                                    // 实时流：推送服务日志增量
                                    streams
                                        .broadcast(&format!("service:{id}"), st.log.clone())
                                        .await;
                                }
                                match map_service_status(&st.status) {
                                    Some("running") => {
                                        let _ = repo.set_status(id, "running", st.pid, None).await;
                                    }
                                    Some("failed") => {
                                        let _ =
                                            repo.set_status(id, "failed", None, st.exit_code).await;
                                    }
                                    Some("stopped") => {
                                        let _ = repo
                                            .set_status(id, "stopped", None, st.exit_code)
                                            .await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                        Some(agent_message::Kind::FileListResult(result)) => {
                            file_list.complete(result).await;
                        }
                        Some(agent_message::Kind::ProcessListResult(result)) => {
                            let rid = result.request_id.clone();
                            query
                                .complete(&rid, QueryResponse::ProcessList(result))
                                .await;
                        }
                        Some(agent_message::Kind::ProcessKillResult(result)) => {
                            let rid = result.request_id.clone();
                            query
                                .complete(&rid, QueryResponse::ProcessKill(result))
                                .await;
                        }
                        Some(agent_message::Kind::NetInfoResult(result)) => {
                            let rid = result.request_id.clone();
                            query.complete(&rid, QueryResponse::NetInfo(result)).await;
                        }
                        Some(agent_message::Kind::Register(_)) => {
                            tracing::warn!(agent_id = %agent_id_inner, "duplicate register ignored");
                        }
                        other => {
                            tracing::debug!(
                                agent_id = %agent_id_inner,
                                ?other,
                                "unhandled message"
                            );
                        }
                    },
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!(agent_id = %agent_id_inner, error = %e, "inbound stream error");
                        break;
                    }
                }
            }
            registry.unregister(&agent_id_inner).await;
            tracing::info!(agent_id = %agent_id_inner, "agent disconnected");
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
