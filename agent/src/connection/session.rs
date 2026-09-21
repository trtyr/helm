//! 一条已建立的双向流会话：持有下行句柄与各执行面 handler，处理 Server 下发的消息。
//!
//! 自 `connection.rs` 拆出（G7：该文件 487 行越界）。父模块负责「连上/断开/重连」，
//! 本模块负责「连上之后这一条流上发生什么」。
//!
//! 分派分两级（与 `forward.rs` 同构）：控制面（执行/文件/终端/服务/自毁）逐条显式列出，
//! 查询面与代理面走 [`AgentSession::handle_aux`]。

use anyhow::{Result, anyhow};
use helm_proto::pb::{AgentMessage, SessionOpened, agent_message, server_message};
use tokio::sync::mpsc;

/// 一条会话的全部可变状态（每连接一个）。
pub struct AgentSession {
    agent_id: String,
    tx: mpsc::Sender<AgentMessage>,
    file: crate::file::FileHandler,
    sessions: crate::pty::SessionManager,
    proxies: crate::proxy::ProxyManager,
    services: crate::service::ServiceManager,
}

impl AgentSession {
    pub fn new(agent_id: String, tx: mpsc::Sender<AgentMessage>) -> Self {
        Self {
            agent_id,
            tx,
            file: crate::file::FileHandler::new(),
            sessions: crate::pty::SessionManager::new(),
            proxies: crate::proxy::ProxyManager::new(),
            services: crate::service::ServiceManager::new(),
        }
    }

    /// 处理一条下行消息。返回 `Err` 表示**会话必须终止**（目前只有注册被拒）。
    pub async fn handle(&mut self, kind: Option<server_message::Kind>) -> Result<()> {
        use server_message::Kind as K;
        let Some(kind) = kind else { return Ok(()) };
        match kind {
            // 控制面
            K::RegisterAck(ack) => return self.on_register_ack(ack),
            K::ExecRequest(req) => self.on_exec_request(req),
            K::JobCancel(req) => crate::exec::request_cancel(&req.job_id),
            K::FileRequest(req) => self.on_file_request(req).await,
            K::FileChunk(chunk) => self.on_file_chunk(chunk).await,
            K::SelfDestruct(sd) => self.on_self_destruct(sd),
            K::SessionOpen(req) => self.on_session_open(req).await,
            K::SessionInput(req) => self.sessions.input(&req.session_id, &req.data),
            K::SessionClose(req) => self.sessions.close(&req.session_id),
            K::SessionResize(req) => {
                self.sessions
                    .resize(&req.session_id, req.cols as u16, req.rows as u16)
            }
            K::ServiceStart(req) => self.on_service_start(req).await,
            K::ServiceStop(req) => self.on_service_stop(req).await,
            // 查询面与代理面
            other => self.handle_aux(other).await,
        }
        Ok(())
    }

    /// 慢通道：8 类「请求-响应」查询、内存扫描与反向代理。
    async fn handle_aux(&mut self, kind: server_message::Kind) {
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
            K::AutorunsAction(req) => {
                self.send_msg(crate::ir::autoruns_action(
                    &req.request_id,
                    &req.action,
                    &req.op_key,
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
            // 未知/后续阶段消息：只留 debug 记录，不影响会话
            other => {
                tracing::debug!(agent_id = %self.agent_id, ?other, "server message (later phase)")
            }
        }
    }

    /// 注册回执：被拒即终止会话（错误向上冒泡到 `connect_once`）。
    fn on_register_ack(&self, ack: helm_proto::pb::RegisterAck) -> Result<()> {
        if ack.ok {
            tracing::info!(
                agent_id = %self.agent_id,
                message = %ack.message,
                "registered"
            );
            return Ok(());
        }
        Err(anyhow!("register rejected: {}", ack.message))
    }

    /// 执行请求：派生任务执行并回报，不阻塞入站循环。
    fn on_exec_request(&self, req: helm_proto::pb::ExecRequest) {
        tracing::info!(job_id = %req.job_id, command = %req.command, "exec request received, spawning");
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
            tracing::info!(job_id = %req.job_id, "exec task finished (result reported)");
        });
    }

    async fn on_file_request(&mut self, req: helm_proto::pb::FileRequest) {
        self.file.handle_request(req, &self.tx).await;
    }

    async fn on_file_chunk(&mut self, chunk: helm_proto::pb::FileChunk) {
        self.file.handle_chunk(chunk, &self.tx).await;
    }

    /// 自毁/卸载指令（不可逆，先留 warn 记录再执行）。
    fn on_self_destruct(&self, sd: helm_proto::pb::SelfDestruct) {
        tracing::warn!(
            agent_id = %self.agent_id,
            remove_binary = sd.remove_binary,
            "uninstall command received"
        );
        crate::uninstall::self_destruct(sd.remove_binary);
    }

    /// 打开交互终端：成功回 `SessionOpened`，失败只告警。
    async fn on_session_open(&self, req: helm_proto::pb::SessionOpen) {
        let tx_out = self.tx.clone();
        match self.sessions.open(
            &req.session_id,
            req.cols as u16,
            req.rows as u16,
            &req.command,
            tx_out.clone(),
        ) {
            Ok(()) => {
                // 对端已断则回执无意义：会话随主循环退出，server 侧由流结束事件记账
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

    async fn on_service_start(&self, req: helm_proto::pb::ServiceStart) {
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

    async fn on_service_stop(&self, req: helm_proto::pb::ServiceStop) {
        self.services.stop(&req.service_id).await;
    }

    /// 内存扫描：流式请求派生任务，一次性请求同步取结果后回发。
    async fn on_mem_scan(&self, req: helm_proto::pb::MemScan) {
        tracing::info!(stream = req.stream, pid = req.pid, kw = %req.keywords, "memscan request received");
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

    /// 发送一条出站消息。
    ///
    /// **错误契约（T3）**：发送失败即对端已断，会话随入站循环退出；server 侧由流结束事件
    /// （`on_disconnect` → status_events `offline` + 离线通知）统一记账。因此这里**刻意不告警**——
    /// 否则每次正常断连都会多一条无信息量的噪音。这就是该 `let _ =` 的安全性依据。
    async fn send_msg(&self, msg: AgentMessage) {
        let _ = self.tx.send(msg).await; // 对端已断 → 会话退出，离线由 server 侧统一记录
    }
}
