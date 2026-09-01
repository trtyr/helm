//! 正向模式：Agent 作为 gRPC server，处理 Server 主动连入的通道。

use std::pin::Pin;

use crate::config::Config;
use anyhow::Result;
use helm_proto::pb::{
    AgentMessage, ServerMessage,
    forward_agent_service_server::{ForwardAgentService, ForwardAgentServiceServer},
    server_message,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::async_trait;
use tonic::{Request, Response, Status, Streaming};

/// 正向模式：启动 gRPC server 监听。
pub async fn serve(config: &Config) -> Result<()> {
    let addr = config.listen_addr.parse()?;
    let svc = ForwardAgentServiceServer::new(ForwardAgentServiceImpl);
    tracing::info!(addr = %config.listen_addr, "agent forward mode listening");
    tonic::transport::Server::builder()
        .add_service(svc)
        .serve(addr)
        .await?;
    Ok(())
}

struct ForwardAgentServiceImpl;

#[async_trait]
impl ForwardAgentService for ForwardAgentServiceImpl {
    type OpenForwardChannelStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<AgentMessage, Status>> + Send>>;

    async fn open_forward_channel(
        &self,
        request: Request<Streaming<ServerMessage>>,
    ) -> Result<Response<Self::OpenForwardChannelStream>, Status> {
        tracing::info!("forward channel request received");
        let mut inbound = request.into_inner();
        let (tx, rx) = mpsc::channel::<AgentMessage>(64);

        tokio::spawn(async move {
            let mut file_handler = crate::file::FileHandler::new();
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
                    _ => {}
                }
            }
        });

        let outbound = ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(outbound)))
    }
}
