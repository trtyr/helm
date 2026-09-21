//! 公共入站消息处理：reverse（Agent 连入）与 forward（Server 拨号）共用的
//! AgentMessage 消费逻辑。两个方向协议同构（决策 002），处理逻辑收敛到此处。
//!
//! 结构（G1/G2，2026-09-20）：`handle` 只做类型分发；每一类消息的处理落在独立的
//! `on_*` 方法里，构造依赖经 [`InboundCtxDeps`] 一次性传入。

use std::collections::HashMap;

use crate::application::agent_lifecycle_service::AgentLifecycleService;
use crate::application::exec_service::ExecService;
use crate::application::service_service::ServiceService;
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::query_registry::{QueryRegistry, QueryResponse};
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use helm_proto::pb::{
    AgentMessage, ExecResult, FileChunk, FileStatus, Heartbeat, MemScanResult, MetricReport,
    ServiceStatus, SessionClosed, SessionOutput, agent_message,
};
use tokio::sync::watch;
use uuid::Uuid;

/// 构造一条入站处理上下文所需的全部依赖。
///
/// 这些依赖由「连接建立」处一次性装配（`agent_service::open_channel` /
/// `forward_manager::connect_once`）——上下文本身不再接受裸参数列表。
pub struct InboundCtxDeps {
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
    /// 指标落库队列（E2）：MetricReport 异步化，不阻塞结果类消息。
    pub metrics: crate::application::metric_sink::MetricSink,
    pub db: Db,
}

/// 一条 agent 连接的入站处理上下文（每连接一个）。
pub struct InboundCtx {
    pub agent_id: String,
    pub host_id: Option<Uuid>,
    pub hostname: String,
    pub registry: ConnectionRegistry,
    pub kick_tx: watch::Sender<bool>,
    pub transfers: TransferRegistry,
    pub sessions: SessionRegistry,
    pub file_list: FileListRegistry,
    pub query: QueryRegistry,
    pub streams: StreamRegistry,
    pub metrics: crate::application::metric_sink::MetricSink,
    pub db: Db,
    /// job_id → 累积输出（ExecResult 分块重组）。
    outputs: HashMap<String, String>,
}

impl InboundCtx {
    pub fn new(deps: InboundCtxDeps) -> Self {
        Self {
            agent_id: deps.agent_id,
            host_id: deps.host_id,
            hostname: deps.hostname,
            registry: deps.registry,
            kick_tx: deps.kick_tx,
            transfers: deps.transfers,
            sessions: deps.sessions,
            file_list: deps.file_list,
            query: deps.query,
            streams: deps.streams,
            metrics: deps.metrics,
            db: deps.db,
            outputs: HashMap::new(),
        }
    }

    /// 消费一条入站消息：仅做类型分发，各阶段逻辑见对应的 `on_*` 方法。
    ///
    /// 分两级：**热路径**（Agent 周期性上报与结果类消息，逐条显式列出）与
    /// **慢通道**（终端/服务/代理/查询结果等，见 [`Self::on_slow_path`]）。
    pub async fn handle(&mut self, msg: AgentMessage) {
        let agent_id = self.agent_id.clone();
        match msg.kind {
            Some(agent_message::Kind::Heartbeat(h)) => self.on_heartbeat(&agent_id, h).await,
            Some(agent_message::Kind::MetricReport(report)) => {
                self.on_metric_report(&agent_id, report).await
            }
            Some(agent_message::Kind::ExecResult(er)) => self.on_exec_result(er).await,
            Some(agent_message::Kind::FileChunk(chunk)) => self.on_file_chunk(chunk).await,
            Some(agent_message::Kind::FileStatus(status)) => self.on_file_status(status).await,
            Some(agent_message::Kind::FileListResult(result)) => {
                self.file_list.complete(result).await;
            }
            Some(agent_message::Kind::MemScanResult(r)) => self.on_mem_scan_result(r).await,
            other => self.on_slow_path(other, &agent_id).await,
        }
    }

    /// 慢通道：终端会话、托管服务、反向代理、8 类「请求-响应」查询结果与重复注册。
    async fn on_slow_path(&mut self, kind: Option<agent_message::Kind>, agent_id: &str) {
        match kind {
            Some(agent_message::Kind::SessionOpened(opened)) => {
                tracing::info!(session_id = %opened.session_id, "session opened");
            }
            Some(agent_message::Kind::SessionOutput(out)) => self.on_session_output(out).await,
            Some(agent_message::Kind::SessionClosed(closed)) => {
                self.on_session_closed(closed).await
            }
            Some(agent_message::Kind::ServiceStatus(st)) => self.on_service_status(st).await,
            Some(agent_message::Kind::ProxyConnected(r)) => {
                crate::grpc::proxy_registry::registry()
                    .connected(&r.conn_id, r.ok, r.error)
                    .await;
            }
            Some(agent_message::Kind::ProxyData(r)) => {
                crate::grpc::proxy_registry::registry()
                    .data(&r.conn_id, r.data)
                    .await;
            }
            Some(agent_message::Kind::ProxyClose(r)) => {
                crate::grpc::proxy_registry::registry()
                    .closed(&r.conn_id)
                    .await;
            }
            Some(agent_message::Kind::Register(_)) => {
                tracing::warn!(agent_id = %agent_id, "duplicate register ignored");
            }
            other => {
                // 「请求-响应」型结果（进程/网络/系统服务/IR/时间线等）统一收敛到
                // QueryRegistry；其余（含未识别的 oneof 变体与 None）只留 debug 记录。
                match other.as_ref().and_then(as_query_response) {
                    Some((rid, resp)) => self.complete_query(&rid, resp).await,
                    None => tracing::debug!(agent_id = %agent_id, ?other, "unhandled message"),
                }
            }
        }
    }

    /// 心跳：刷新 `agents.last_heartbeat`。
    async fn on_heartbeat(&self, agent_id: &str, h: Heartbeat) {
        if let Err(e) = AgentLifecycleService::new(self.db.clone(), self.registry.clone())
            .update_heartbeat(agent_id, h.timestamp_unix_ms)
            .await
        {
            tracing::warn!(agent_id = %agent_id, error = %e, "failed to update heartbeat");
        }
        tracing::debug!(agent_id = %agent_id, ts_ms = h.timestamp_unix_ms, "heartbeat");
    }

    /// 指标上报：只做有界入队，落库/告警/广播在 metric sink 任务内完成（E2）。
    async fn on_metric_report(&self, agent_id: &str, report: MetricReport) {
        let count = report.metrics.len();
        // E2：入站只做有界入队，ExecResult / FileStatus 等结果类消息不再被逐条 INSERT 拖住
        if let Some(host_id) = self.host_id {
            self.metrics.enqueue(host_id, report.metrics).await;
        }
        tracing::debug!(agent_id = %agent_id, count, "metrics received");
    }

    /// 执行结果：分块累积输出，最终帧落库 job 终态。
    async fn on_exec_result(&mut self, er: ExecResult) {
        let job_id = er.job_id.clone();
        let entry = self.outputs.entry(job_id.clone()).or_default();
        if let Some(chunk) = er.chunk {
            entry.push_str(&String::from_utf8_lossy(&chunk.data));
            // 实时流：推送 job 输出增量
            self.streams
                .broadcast(&format!("job:{job_id}"), chunk.data)
                .await;
        }
        if !er.finished {
            return;
        }
        let mut output = self.outputs.remove(&job_id).unwrap_or_default();
        // B6：agent 侧输出超限截断时在库内留痕
        if er.truncated {
            output.push_str("\n…[输出超过上限被截断]");
        }
        let status = crate::grpc::agent_service::job_status(
            er.error.as_deref(),
            er.exit_code,
            er.cancelled,
            er.timed_out,
        );
        match Uuid::parse_str(&job_id) {
            Ok(id) => match ExecService::new(self.db.clone(), self.registry.clone())
                .finish_job(id, status, &output, er.exit_code)
                .await
            {
                Ok(written) => {
                    // written=false 表示终态已被幂等守卫拦下（重放/迟到回报），不是错误
                    tracing::info!(job_id = %job_id, status, written, "job finished")
                }
                Err(e) => {
                    tracing::warn!(job_id = %job_id, error = %e, "failed to persist job result")
                }
            },
            Err(_) => tracing::warn!(job_id = %job_id, "invalid job_id"),
        }
    }

    /// 文件传输分片：累加到对应 transfer。
    async fn on_file_chunk(&self, chunk: FileChunk) {
        self.transfers
            .accumulate_chunk(&chunk.transfer_id, &chunk.data)
            .await;
    }

    /// 文件传输收尾状态。
    async fn on_file_status(&self, status: FileStatus) {
        let transfer_id = status.transfer_id.clone();
        self.transfers.complete(&transfer_id, status).await;
    }

    /// 终端输出转发到 PTY 会话。
    async fn on_session_output(&self, out: SessionOutput) {
        self.sessions.forward(&out.session_id, out.data).await;
    }

    /// 终端会话结束。
    async fn on_session_closed(&self, closed: SessionClosed) {
        self.sessions.unregister(&closed.session_id).await;
        tracing::info!(session_id = %closed.session_id, "session closed");
    }

    /// 托管服务状态上报：落库 + 两个实时流（`service:{id}` 日志、`services` 状态）。
    async fn on_service_status(&self, st: ServiceStatus) {
        let Ok(id) = Uuid::parse_str(&st.service_id) else {
            return;
        };
        let svc = ServiceService::new(self.db.clone(), self.registry.clone());
        if !st.log.is_empty() {
            // G5 残余说明：日志追加失败只影响历史可读性，实时流仍会推给控制台，
            // 故此处保持非致命（不中断入站循环）。
            if let Err(e) = svc.append_log(id, &st.log).await {
                tracing::warn!(service_id = %id, error = %e, "service log append failed");
            }
            // 实时流：推送服务日志增量
            self.streams
                .broadcast(&format!("service:{id}"), st.log.clone())
                .await;
        }
        let mapped = crate::grpc::agent_service::map_service_status(&st.status);
        if let Some(status) = mapped {
            // G5 残余说明：状态写入失败会记 error（此前完全静默），但不中断入站循环——
            // 后续 ServiceStatus 上报或控制台查询仍可纠正视图。
            if let Err(e) = svc.report_status(id, status, st.pid, st.exit_code).await {
                tracing::error!(
                    service_id = %id,
                    status,
                    error = %e,
                    "service status persist failed"
                );
            }
        }
        // D4：服务状态流——有状态变化即广播（含 agent 上报的原始状态）
        if mapped.is_some() {
            let payload = serde_json::json!({
                "service_id": id,
                "status": mapped,
                "pid": st.pid,
                "exit_code": st.exit_code,
            });
            self.streams
                .broadcast("services", payload.to_string().into_bytes())
                .await;
        }
    }

    /// 通用「请求-响应」型结果的收敛点（进程/网络/系统服务/IR/时间线等）。
    async fn complete_query(&self, request_id: &str, resp: QueryResponse) {
        self.query.complete(request_id, resp).await;
    }
    /// 内存扫描：每一帧都广播给 WS 订阅者；最终帧额外完成挂起的查询。
    async fn on_mem_scan_result(&self, result: MemScanResult) {
        let rid = result.request_id.clone();
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

    /// 连接结束：身份校验式注销 + 下线通知 + 状态事件落库（断连即发，决策 009 / P003 T1）。
    /// 被顶掉的旧连接（重复注册后 kick）退出时身份不匹配：不注销、不发通知、不落库。
    pub async fn on_disconnect(&self, reason: &str, detail: &str) {
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
        tracing::info!(agent_id = %self.agent_id, reason, "agent disconnected");
        if let Some(host_id) = self.host_id {
            // P003 T1：状态事件落库（与通知同点同语义；detail 截断防超长 error 链）
            let detail = detail.chars().take(500).collect::<String>();
            crate::application::notification_service::record_status_event(
                &self.db, host_id, "offline", reason, &detail,
            )
            .await;
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

/// 「请求-响应」型结果消息的统一识别与转换：取出 `request_id` 并包装为 [`QueryResponse`]。
///
/// 返回 `None` 表示该消息不属于查询结果类（由调用方按「未处理」记录）。
/// 这类消息共 8 种（进程 / 网络 / 系统服务 / IR / 时间线），语义完全一致，
/// 因此收敛到一处而不是展开成 8 个一模一样的分发分支（G1）。
fn as_query_response(kind: &agent_message::Kind) -> Option<(String, QueryResponse)> {
    use agent_message::Kind as K;
    Some(match kind {
        K::ProcessListResult(r) => (r.request_id.clone(), QueryResponse::ProcessList(r.clone())),
        K::ProcessKillResult(r) => (r.request_id.clone(), QueryResponse::ProcessKill(r.clone())),
        K::NetInfoResult(r) => (r.request_id.clone(), QueryResponse::NetInfo(r.clone())),
        K::SysServiceListResult(r) => (
            r.request_id.clone(),
            QueryResponse::SysServiceList(r.clone()),
        ),
        K::SysServiceActionResult(r) => (
            r.request_id.clone(),
            QueryResponse::SysServiceAction(r.clone()),
        ),
        K::IrScanResult(r) => (r.request_id.clone(), QueryResponse::IrScan(r.clone())),
        K::AutorunsActionResult(r) => (
            r.request_id.clone(),
            QueryResponse::AutorunsAction(r.clone()),
        ),
        K::FsTimelineResult(r) => (r.request_id.clone(), QueryResponse::FsTimeline(r.clone())),
        _ => return None,
    })
}
