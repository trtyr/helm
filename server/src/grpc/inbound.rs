//! 公共入站消息处理：reverse（Agent 连入）与 forward（Server 拨号）共用的
//! AgentMessage 消费逻辑。两个方向协议同构（决策 002），处理逻辑收敛到此处。

use std::collections::HashMap;

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::query_registry::{QueryRegistry, QueryResponse};
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::alert_repo::AlertRepo;
use crate::store::service_repo::ServiceRepo;
use crate::store::{Db, agent_repo::AgentRepo, job_repo::JobRepo, metric_repo::MetricRepo};
use helm_proto::pb::{AgentMessage, agent_message};
use tokio::sync::watch;
use uuid::Uuid;

/// 一条 agent 连接的入站处理上下文（每连接一个）。
pub struct InboundCtx {
    pub agent_id: String,
    pub host_id: Option<Uuid>,
    /// 主机名（通知文案用，注册时上报）。
    pub hostname: String,
    pub registry: ConnectionRegistry,
    /// 本连接身份（注册表比对用）：被顶掉的旧连接断开时不得注销新连接。
    pub kick_tx: watch::Sender<bool>,
    pub transfers: TransferRegistry,
    pub sessions: SessionRegistry,
    pub file_list: FileListRegistry,
    pub query: QueryRegistry,
    pub streams: StreamRegistry,
    pub db: Db,
    /// job_id → 累积输出（ExecResult 分块重组）。
    outputs: HashMap<String, String>,
}

impl InboundCtx {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        agent_id: String,
        host_id: Option<Uuid>,
        hostname: String,
        registry: ConnectionRegistry,
        kick_tx: watch::Sender<bool>,
        transfers: TransferRegistry,
        sessions: SessionRegistry,
        file_list: FileListRegistry,
        query: QueryRegistry,
        streams: StreamRegistry,
        db: Db,
    ) -> Self {
        Self {
            agent_id,
            host_id,
            hostname,
            registry,
            kick_tx,
            transfers,
            sessions,
            file_list,
            query,
            streams,
            db,
            outputs: HashMap::new(),
        }
    }

    /// 消费一条入站消息。
    pub async fn handle(&mut self, msg: AgentMessage) {
        let agent_id = self.agent_id.clone();
        match msg.kind {
            Some(agent_message::Kind::Heartbeat(h)) => {
                if let Err(e) = AgentRepo::new(self.db.clone())
                    .update_heartbeat(&agent_id, h.timestamp_unix_ms)
                    .await
                {
                    tracing::warn!(agent_id = %agent_id, error = %e, "failed to update heartbeat");
                }
                tracing::debug!(agent_id = %agent_id, ts_ms = h.timestamp_unix_ms, "heartbeat");
            }
            Some(agent_message::Kind::MetricReport(report)) => {
                let count = report.metrics.len();
                if let Some(host_id) = self.host_id {
                    let metric_repo = MetricRepo::new(self.db.clone());
                    let alert_repo = AlertRepo::new(self.db.clone());
                    for m in report.metrics {
                        let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                            m.timestamp_unix_ms as i64,
                        )
                        .unwrap_or_else(chrono::Utc::now);
                        if let Err(e) = metric_repo.insert(host_id, &m.name, m.value, ts).await {
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
                            // 预警联动通知中心（决策 009：系统内小卡片）
                            let svc =
                                crate::application::notification_service::NotificationService::new(
                                    self.db.clone(),
                                    self.streams.clone(),
                                );
                            let _ = svc
                                .notify(
                                    host_id,
                                    crate::application::notification_service::KIND_ALERT,
                                    &format!(
                                        "预警：{} = {:.1}（阈值 {}）",
                                        m.name, m.value, threshold
                                    ),
                                )
                                .await;
                        }
                        // 实时流：推送指标
                        let payload = serde_json::json!({
                            "host_id": host_id,
                            "name": m.name,
                            "value": m.value,
                            "ts": m.timestamp_unix_ms,
                        });
                        self.streams
                            .broadcast("metrics", payload.to_string().into_bytes())
                            .await;
                    }
                }
                tracing::debug!(agent_id = %agent_id, count, "metrics received");
            }
            Some(agent_message::Kind::ExecResult(er)) => {
                let job_id = er.job_id.clone();
                let entry = self.outputs.entry(job_id.clone()).or_default();
                if let Some(chunk) = er.chunk {
                    entry.push_str(&String::from_utf8_lossy(&chunk.data));
                    // 实时流：推送 job 输出增量
                    self.streams
                        .broadcast(&format!("job:{job_id}"), chunk.data)
                        .await;
                }
                if er.finished {
                    let output = self.outputs.remove(&job_id).unwrap_or_default();
                    let status =
                        crate::grpc::agent_service::job_status(er.error.as_deref(), er.exit_code);
                    match Uuid::parse_str(&job_id) {
                        Ok(id) => {
                            if let Err(e) = JobRepo::new(self.db.clone())
                                .finish(id, status, &output, er.exit_code)
                                .await
                            {
                                tracing::warn!(job_id = %job_id, error = %e, "failed to persist job result");
                            } else {
                                tracing::info!(job_id = %job_id, status, "job finished");
                            }
                        }
                        Err(_) => tracing::warn!(job_id = %job_id, "invalid job_id"),
                    }
                }
            }
            Some(agent_message::Kind::FileChunk(chunk)) => {
                self.transfers
                    .accumulate_chunk(&chunk.transfer_id, &chunk.data)
                    .await;
            }
            Some(agent_message::Kind::FileStatus(status)) => {
                let tid = status.transfer_id.clone();
                self.transfers.complete(&tid, status).await;
            }
            Some(agent_message::Kind::SessionOpened(opened)) => {
                tracing::info!(session_id = %opened.session_id, "session opened");
            }
            Some(agent_message::Kind::SessionOutput(out)) => {
                let _ = self.sessions.forward(&out.session_id, out.data).await;
            }
            Some(agent_message::Kind::SessionClosed(closed)) => {
                self.sessions.unregister(&closed.session_id).await;
                tracing::info!(session_id = %closed.session_id, "session closed");
            }
            Some(agent_message::Kind::ServiceStatus(st)) => {
                if let Ok(id) = Uuid::parse_str(&st.service_id) {
                    let repo = ServiceRepo::new(self.db.clone());
                    if !st.log.is_empty() {
                        let _ = repo.append_log(id, &st.log).await;
                        // 实时流：推送服务日志增量
                        self.streams
                            .broadcast(&format!("service:{id}"), st.log.clone())
                            .await;
                    }
                    match crate::grpc::agent_service::map_service_status(&st.status) {
                        Some("running") => {
                            let _ = repo.set_status(id, "running", st.pid, None).await;
                        }
                        Some("failed") => {
                            let _ = repo.set_status(id, "failed", None, st.exit_code).await;
                        }
                        Some("stopped") => {
                            let _ = repo.set_status(id, "stopped", None, st.exit_code).await;
                        }
                        _ => {}
                    }
                }
            }
            Some(agent_message::Kind::FileListResult(result)) => {
                self.file_list.complete(result).await;
            }
            Some(agent_message::Kind::ProcessListResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::ProcessList(result))
                    .await;
            }
            Some(agent_message::Kind::ProcessKillResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::ProcessKill(result))
                    .await;
            }
            Some(agent_message::Kind::NetInfoResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::NetInfo(result))
                    .await;
            }
            Some(agent_message::Kind::SysServiceListResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::SysServiceList(result))
                    .await;
            }
            Some(agent_message::Kind::SysServiceActionResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::SysServiceAction(result))
                    .await;
            }
            Some(agent_message::Kind::IrScanResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::IrScan(result))
                    .await;
            }
            Some(agent_message::Kind::MemScanResult(result)) => {
                let rid = result.request_id.clone();
                // 流式扫描：所有帧都广播给 WS 订阅者；最终帧额外完成挂起的查询
                let payload = serde_json::json!({
                    "scanId": rid,
                    "finished": result.finished,
                    "pid": result.pid,
                    "matches": result.matches,
                    "hits": result
                        .hits
                        .iter()
                        .map(|h| serde_json::json!({"addr": h.addr, "kind": h.kind, "value": h.value}))
                        .collect::<Vec<_>>(),
                    "scannedBytes": result.scanned_bytes,
                    "pidsTotal": result.pids_total,
                    "pidsScanned": result.pids_scanned,
                    "truncated": result.truncated,
                    "timedOut": result.timed_out,
                    "error": result.error,
                })
                .to_string();
                tracing::debug!(rid = %rid, finished = result.finished, n = result.matches.len(), "memscan frame");
                self.streams
                    .broadcast(&format!("memscan:{rid}"), payload.into_bytes())
                    .await;
                if result.finished {
                    self.query
                        .complete(&rid, QueryResponse::MemScan(result))
                        .await;
                }
            }
            Some(agent_message::Kind::AutorunsActionResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::AutorunsAction(result))
                    .await;
            }
            Some(agent_message::Kind::FsTimelineResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::FsTimeline(result))
                    .await;
            }
            Some(agent_message::Kind::FileMetaResult(result)) => {
                let rid = result.request_id.clone();
                self.query
                    .complete(&rid, QueryResponse::FileMeta(result))
                    .await;
            }
            Some(agent_message::Kind::ProxyConnected(result)) => {
                crate::grpc::proxy_registry::registry()
                    .connected(&result.conn_id, result.ok, result.error)
                    .await;
            }
            Some(agent_message::Kind::ProxyData(result)) => {
                crate::grpc::proxy_registry::registry()
                    .data(&result.conn_id, result.data)
                    .await;
            }
            Some(agent_message::Kind::ProxyClose(result)) => {
                crate::grpc::proxy_registry::registry()
                    .closed(&result.conn_id)
                    .await;
            }
            Some(agent_message::Kind::Register(_)) => {
                tracing::warn!(agent_id = %agent_id, "duplicate register ignored");
            }
            other => {
                tracing::debug!(agent_id = %agent_id, ?other, "unhandled message");
            }
        }
    }

    /// 连接结束：身份校验式注销 + 下线通知（断连即发，决策 009）。
    /// 被顶掉的旧连接（重复注册后 kick）退出时身份不匹配：不注销、不发通知。
    pub async fn on_disconnect(&self) {
        if !self
            .registry
            .unregister_if_current(&self.agent_id, &self.kick_tx)
            .await
        {
            tracing::debug!(
                agent_id = %self.agent_id,
                "stale connection released; agent owned by newer registration"
            );
            return;
        }
        tracing::info!(agent_id = %self.agent_id, "agent disconnected");
        if let Some(host_id) = self.host_id {
            let svc = crate::application::notification_service::NotificationService::new(
                self.db.clone(),
                self.streams.clone(),
            );
            if let Err(e) = svc
                .notify(
                    host_id,
                    crate::application::notification_service::KIND_OFFLINE,
                    &format!("主机 {} 已下线", self.hostname),
                )
                .await
            {
                tracing::warn!(agent_id = %self.agent_id, error = ?e, "offline notify failed");
            }
        }
    }
}
