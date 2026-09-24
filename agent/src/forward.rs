//! 正向模式：Agent 作为 gRPC server，处理 Server 主动连入的通道。
//!
//! 流上协议与 reverse 同构（决策 002）：连接建立后 Agent 先发 `Register`
//! 完成身份上报，随后持续发送心跳与指标；Server 侧据将其注册进
//! ConnectionRegistry，全部控制端点对 forward 主机可用。
//!
//! 结构（G1，2026-09-20）：`open_forward_channel` 只做通道建立；一条连接的全部
//! 状态与入站处理收敛在 [`ForwardSession`] 内，按消息域拆成 `on_*` 方法。

use std::pin::Pin;

use crate::config::Config;
use anyhow::Result;
use helm_proto::pb::{
    AgentMessage, ExecRequest, FileChunk, FileRequest, Heartbeat, MemScan, SelfDestruct,
    ServerMessage, ServiceStart, ServiceStop, SessionOpen, SessionOpened, agent_message,
    forward_agent_service_server::{ForwardAgentService, ForwardAgentServiceServer},
    server_message,
};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::async_trait;
use tonic::{Request, Response, Status, Streaming};

/// 心跳间隔（与 reverse 保持一致）。
const HEARTBEAT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(10);

/// 正向模式：启动 gRPC server 监听。
///
/// `cert` 为 Some 时启用 mTLS（要求客户端出示 CA 签发的证书，双向认证）。
pub async fn serve(config: &Config, cert: Option<crate::cert::AgentCert>) -> Result<()> {
    let addr = config.listen_addr.parse()?;
    let svc = ForwardAgentServiceServer::new(ForwardAgentServiceImpl {
        cfg: config.clone(),
    });
    let mut builder = tonic::transport::Server::builder();
    if let Some(c) = cert {
        use tonic::transport::{Certificate, Identity, ServerTlsConfig};
        let tls = ServerTlsConfig::new()
            .identity(Identity::from_pem(c.cert_pem, c.key_pem))
            .client_ca_root(Certificate::from_pem(c.ca_pem));
        builder = builder.tls_config(tls)?;
        tracing::info!(addr = %config.listen_addr, "agent forward mode listening (mTLS)");
    } else {
        tracing::info!(addr = %config.listen_addr, "agent forward mode listening");
    }
    // B8：forward gRPC server 对称开启 h2 keepalive（与 listener_registry 的 server 端一致）
    let mut builder = builder
        .http2_keepalive_interval(Some(std::time::Duration::from_secs(30)))
        .http2_keepalive_timeout(Some(std::time::Duration::from_secs(10)));
    builder.add_service(svc).serve(addr).await?;
    Ok(())
}

struct ForwardAgentServiceImpl {
    cfg: Config,
}

#[async_trait]
impl ForwardAgentService for ForwardAgentServiceImpl {
    type OpenForwardChannelStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<AgentMessage, Status>> + Send>>;

    /// 建立一条 forward 通道：立即返回出站流，会话在后台任务里跑。
    async fn open_forward_channel(
        &self,
        request: Request<Streaming<ServerMessage>>,
    ) -> Result<Response<Self::OpenForwardChannelStream>, Status> {
        tracing::info!("forward channel request received");
        let (tx, rx) = mpsc::channel::<AgentMessage>(64);
        let session = ForwardSession::new(self.cfg.clone(), tx);
        tokio::spawn(session.run(request.into_inner()));
        let outbound = ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(outbound)))
    }
}

/// 一条 forward 通道的会话状态（每连接一个）。
struct ForwardSession {
    cfg: Config,
    tx: mpsc::Sender<AgentMessage>,
    file: crate::file::FileHandler,
    sessions: crate::pty::SessionManager,
    proxies: crate::proxy::ProxyManager,
    services: crate::service::ServiceManager,
}

impl ForwardSession {
    fn new(cfg: Config, tx: mpsc::Sender<AgentMessage>) -> Self {
        Self {
            cfg,
            tx,
            file: crate::file::FileHandler::new(),
            sessions: crate::pty::SessionManager::new(),
            proxies: crate::proxy::ProxyManager::new(),
            services: crate::service::ServiceManager::new(),
        }
    }

    /// 会话主循环：Register → 起心跳/监控 → 泵入站 → 收尾。
    async fn run(mut self, mut inbound: Streaming<ServerMessage>) {
        if !self.send_register().await {
            return;
        }
        let heartbeat = spawn_heartbeat(self.tx.clone());
        let monitor = spawn_monitor(self.tx.clone());
        while let Ok(Some(msg)) = inbound.message().await {
            tracing::debug!(kind = ?msg.kind, "forward inbound message");
            self.dispatch(msg.kind).await;
        }
        // 流结束：停心跳与监控
        heartbeat.abort();
        monitor.abort();
    }

    /// 首条消息：Register（协议与 reverse 同构，Server 据此注册）。
    /// 发送失败说明对端已断，会话不必继续。
    async fn send_register(&self) -> bool {
        let register = crate::connection::build_register(&self.cfg);
        tracing::info!(agent_id = %self.cfg.agent_id, "forward channel: sending register");
        self.tx
            .send(AgentMessage {
                kind: Some(agent_message::Kind::Register(register)),
            })
            .await
            .is_ok()
    }

    /// 入站分发：控制面消息逐条显式列出；查询/代理面走 [`Self::dispatch_aux`]。
    async fn dispatch(&mut self, kind: Option<server_message::Kind>) {
        use server_message::Kind as K;
        let Some(kind) = kind else { return };
        match kind {
            // 控制面：执行、文件、终端、托管服务、自毁——每条都是一次有副作用的动作
            K::ExecRequest(req) => self.on_exec_request(req),
            K::JobCancel(req) => crate::exec::request_cancel(&req.job_id),
            K::FileRequest(req) => self.on_file_request(req).await,
            K::FileChunk(chunk) => self.on_file_chunk(chunk).await,
            K::SelfDestruct(sd) => on_self_destruct(sd),
            K::SessionOpen(req) => self.on_session_open(req).await,
            K::SessionInput(req) => self.sessions.input(&req.session_id, &req.data),
            K::SessionClose(req) => self.sessions.close(&req.session_id),
            K::SessionResize(req) => {
                self.sessions
                    .resize(&req.session_id, req.cols as u16, req.rows as u16)
            }
            K::ServiceStart(req) => self.on_service_start(req).await,
            K::ServiceStop(req) => self.on_service_stop(req).await,
            // 查询面与代理面：请求-响应型，统一走慢通道
            other => self.dispatch_aux(other).await,
        }
    }

    /// 慢通道：9 类「请求-响应」查询、内存扫描与反向代理。
    async fn dispatch_aux(&mut self, kind: server_message::Kind) {
        use server_message::Kind as K;
        match kind {
            K::FileList(req) => {
                self.send_msg(crate::fs::list_dir(&req.request_id, &req.path))
                    .await
            }
            K::ProcessList(req) => {
                self.send_msg(crate::process::list_processes(&req.request_id))
                    .await
            }
            K::ProcessKill(req) => {
                self.send_msg(crate::process::kill_process(&req.request_id, req.pid))
                    .await
            }
            K::NetInfo(req) => {
                self.send_msg(crate::process::net_info(&req.request_id))
                    .await
            }
            K::SysServiceList(req) => {
                self.send_msg(crate::sys_service::list_services(&req.request_id))
                    .await
            }
            K::SysServiceAction(req) => {
                self.send_msg(crate::sys_service::service_action(
                    &req.request_id,
                    &req.name,
                    &req.action,
                ))
                .await
            }
            K::AutorunsAction(req) => {
                self.send_msg(crate::ir::autoruns_action(
                    &req.request_id,
                    &req.action,
                    &req.op_key,
                ))
                .await
            }
            K::IrScan(req) => {
                self.send_msg(crate::ir::ir_scan(&req.request_id, &req.types))
                    .await
            }
            K::FsTimelineQuery(req) => {
                self.send_msg(crate::ir::fs_timeline(
                    &req.request_id,
                    &req.drive,
                    req.since_hours,
                    req.limit,
                    &req.keyword,
                ))
                .await
            }
            K::MemScan(req) => self.on_mem_scan(req).await,
            K::ProxyConnect(req) => {
                self.proxies
                    .connect(&req.conn_id, &req.target, &self.tx)
                    .await
            }
            K::ProxyData(req) => self.proxies.data(&req.conn_id, &req.data).await,
            K::ProxyClose(req) => self.proxies.close(&req.conn_id).await,
            _ => {}
        }
    }

    /// 发送一条出站消息。
    ///
    /// **错误契约（T3）**：发送失败即对端（Server）已断，会话随入站循环退出；server 侧由
    /// 流结束事件统一记账。因此这里**刻意不告警**（避免正常断连的噪音）。
    /// 这就是该 `let _ =` 的安全性依据。
    async fn send_msg(&self, msg: AgentMessage) {
        let _ = self.tx.send(msg).await; // 对端已断 → 会话退出，离线由 server 侧统一记录
    }

    /// 执行请求：派生任务执行并回报，不阻塞入站循环。
    fn on_exec_request(&self, req: ExecRequest) {
        tracing::info!(job_id = %req.job_id, command = %req.command, "forward exec request");
        let tx = self.tx.clone();
        tokio::spawn(async move {
            crate::exec::run_and_report(
                &req.job_id,
                &req.command,
                &req.args,
                req.timeout_secs,
                &tx,
            )
            .await;
        });
    }

    async fn on_file_request(&mut self, req: FileRequest) {
        self.file.handle_request(req, &self.tx).await;
    }

    async fn on_file_chunk(&mut self, chunk: FileChunk) {
        self.file.handle_chunk(chunk, &self.tx).await;
    }

    /// 打开交互终端：成功回 `SessionOpened`，失败只告警。
    async fn on_session_open(&self, req: SessionOpen) {
        let tx_out = self.tx.clone();
        match self.sessions.open(
            &req.session_id,
            req.cols as u16,
            req.rows as u16,
            &req.command,
            tx_out.clone(),
        ) {
            Ok(()) => {
                // 对端已断则回执无意义：会话随主循环退出
                let _ = tx_out
                    .send(AgentMessage {
                        kind: Some(agent_message::Kind::SessionOpened(SessionOpened {
                            session_id: req.session_id.clone(),
                        })),
                    })
                    .await;
            }
            Err(e) => {
                tracing::warn!(session_id = %req.session_id, error = %e, "session open failed")
            }
        }
    }

    async fn on_service_start(&self, req: ServiceStart) {
        self.services
            .start(
                &req.service_id,
                &req.command,
                &req.args,
                &req.restart_policy,
                self.tx.clone(),
            )
            .await;
    }

    async fn on_service_stop(&self, req: ServiceStop) {
        self.services.stop(&req.service_id).await;
    }

    /// 内存扫描：流式请求派生任务，一次性请求同步取结果后回发。
    async fn on_mem_scan(&self, req: MemScan) {
        if req.stream {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                crate::ir::mem_scan_stream(req.request_id, req.pid, req.min_len, req.keywords, tx)
                    .await;
            });
        } else {
            let msg =
                crate::ir::mem_scan(&req.request_id, req.pid, req.min_len, &req.keywords).await;
            self.send_msg(msg).await;
        }
    }
}

/// 自毁/卸载指令（不可逆，先留 warn 记录再执行）。
fn on_self_destruct(sd: SelfDestruct) {
    tracing::warn!(
        remove_binary = sd.remove_binary,
        "uninstall command received (forward)"
    );
    crate::uninstall::self_destruct(sd.remove_binary);
}

/// 心跳 task：连接断开（tx 关闭）即自行退出。
fn spawn_heartbeat(tx: mpsc::Sender<AgentMessage>) -> JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(HEARTBEAT_INTERVAL).await;
            let msg = AgentMessage {
                kind: Some(agent_message::Kind::Heartbeat(Heartbeat {
                    timestamp_unix_ms: crate::connection::now_ms(),
                })),
            };
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    })
}

/// 监控 task（指标上报）。
fn spawn_monitor(tx: mpsc::Sender<AgentMessage>) -> JoinHandle<()> {
    tokio::spawn(async move {
        crate::monitor::run_monitor(tx).await;
    })
}
