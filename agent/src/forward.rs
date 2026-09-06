//! 正向模式：Agent 作为 gRPC server，处理 Server 主动连入的通道。
//!
//! 流上协议与 reverse 同构（决策 002）：连接建立后 Agent 先发 `Register`
//! 完成身份上报，随后持续发送心跳与指标；Server 侧据将其注册进
//! ConnectionRegistry，全部控制端点对 forward 主机可用。

use std::pin::Pin;

use crate::config::Config;
use anyhow::Result;
use helm_proto::pb::{
    AgentMessage, Heartbeat, ServerMessage, SessionOpened, agent_message,
    forward_agent_service_server::{ForwardAgentService, ForwardAgentServiceServer},
    server_message,
};
use tokio::sync::mpsc;
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

    async fn open_forward_channel(
        &self,
        request: Request<Streaming<ServerMessage>>,
    ) -> Result<Response<Self::OpenForwardChannelStream>, Status> {
        tracing::info!("forward channel request received");
        let cfg = self.cfg.clone();
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel::<AgentMessage>(64);

        tokio::spawn(async move {
            // 1. 先发 Register（协议与 reverse 同构，Server 据此注册）
            let register = crate::connection::build_register(&cfg);
            tracing::info!(agent_id = %cfg.agent_id, "forward channel: sending register");
            if tx
                .send(AgentMessage {
                    kind: Some(agent_message::Kind::Register(register)),
                })
                .await
                .is_err()
            {
                return;
            }

            // 2. 心跳 task（连接断开随 tx drop 退出）
            let tx_hb = tx.clone();
            let heartbeat = tokio::spawn(async move {
                loop {
                    tokio::time::sleep(HEARTBEAT_INTERVAL).await;
                    let msg = AgentMessage {
                        kind: Some(agent_message::Kind::Heartbeat(Heartbeat {
                            timestamp_unix_ms: crate::connection::now_ms(),
                        })),
                    };
                    if tx_hb.send(msg).await.is_err() {
                        break;
                    }
                }
            });

            // 3. 监控 task（指标上报）
            let tx_mon = tx.clone();
            let monitor = tokio::spawn(async move {
                crate::monitor::run_monitor(tx_mon).await;
            });

            // 4. 入站处理循环
            let mut file_handler = crate::file::FileHandler::new();
            let sessions = crate::pty::SessionManager::new();
            let services = crate::service::ServiceManager::new();
            while let Ok(Some(msg)) = inbound.message().await {
                tracing::debug!(kind = ?msg.kind, "forward inbound message");
                match msg.kind {
                    Some(server_message::Kind::ExecRequest(req)) => {
                        tracing::info!(job_id = %req.job_id, command = %req.command, "forward exec request");
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            crate::exec::run_and_report(&req.job_id, &req.command, &req.args, &tx)
                                .await;
                        });
                    }
                    Some(server_message::Kind::FileRequest(req)) => {
                        file_handler.handle_request(req, &tx).await;
                    }
                    Some(server_message::Kind::FileChunk(chunk)) => {
                        file_handler.handle_chunk(chunk, &tx).await;
                    }
                    Some(server_message::Kind::SelfDestruct(sd)) => {
                        tracing::warn!(
                            remove_binary = sd.remove_binary,
                            "uninstall command received (forward)"
                        );
                        crate::uninstall::self_destruct(sd.remove_binary);
                    }
                    Some(server_message::Kind::SessionOpen(req)) => {
                        let tx_out = tx.clone();
                        match sessions.open(
                            &req.session_id,
                            req.cols as u16,
                            req.rows as u16,
                            &req.command,
                            tx_out.clone(),
                        ) {
                            Ok(()) => {
                                let _ = tx_out
                                    .send(AgentMessage {
                                        kind: Some(agent_message::Kind::SessionOpened(
                                            SessionOpened {
                                                session_id: req.session_id.clone(),
                                            },
                                        )),
                                    })
                                    .await;
                            }
                            Err(e) => {
                                tracing::warn!(session_id = %req.session_id, error = %e, "session open failed");
                            }
                        }
                    }
                    Some(server_message::Kind::SessionInput(req)) => {
                        sessions.input(&req.session_id, &req.data);
                    }
                    Some(server_message::Kind::SessionClose(req)) => {
                        sessions.close(&req.session_id);
                    }
                    Some(server_message::Kind::SessionResize(req)) => {
                        sessions.resize(&req.session_id, req.cols as u16, req.rows as u16);
                    }
                    Some(server_message::Kind::ServiceStart(req)) => {
                        services
                            .start(
                                &req.service_id,
                                &req.command,
                                &req.args,
                                &req.restart_policy,
                                tx.clone(),
                            )
                            .await;
                    }
                    Some(server_message::Kind::ServiceStop(req)) => {
                        services.stop(&req.service_id).await;
                    }
                    Some(server_message::Kind::FileList(req)) => {
                        let msg = crate::fs::list_dir(&req.request_id, &req.path);
                        let _ = tx.send(msg).await;
                    }
                    Some(server_message::Kind::ProcessList(req)) => {
                        let msg = crate::process::list_processes(&req.request_id);
                        let _ = tx.send(msg).await;
                    }
                    Some(server_message::Kind::ProcessKill(req)) => {
                        let msg = crate::process::kill_process(&req.request_id, req.pid);
                        let _ = tx.send(msg).await;
                    }
                    Some(server_message::Kind::NetInfo(req)) => {
                        let msg = crate::process::net_info(&req.request_id);
                        let _ = tx.send(msg).await;
                    }
                    Some(server_message::Kind::SysServiceList(req)) => {
                        let msg = crate::sys_service::list_services(&req.request_id);
                        let _ = tx.send(msg).await;
                    }
                    Some(server_message::Kind::SysServiceAction(req)) => {
                        let msg = crate::sys_service::service_action(
                            &req.request_id,
                            &req.name,
                            &req.action,
                        );
                        let _ = tx.send(msg).await;
                    }
                    _ => {}
                }
            }

            // 流结束：停心跳与监控
            heartbeat.abort();
            monitor.abort();
        });

        let outbound = ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(outbound)))
    }
}
