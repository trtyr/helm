//! 应用层：正向连接（Server 主动连 Agent）。

use crate::domain::{Error, Result};
use helm_proto::pb::{
    ExecRequest, ServerMessage, agent_message,
    forward_agent_service_client::ForwardAgentServiceClient, server_message,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;
use uuid::Uuid;

/// 正向连接用例（无状态）。
#[derive(Clone, Default)]
pub struct ForwardService;

impl ForwardService {
    pub fn new() -> Self {
        Self
    }

    /// 正向连接：拨号 Agent，下发命令，收集输出与退出码。
    pub async fn exec(
        &self,
        agent_addr: &str,
        command: &str,
        args: &[String],
    ) -> Result<(String, Option<i32>)> {
        let channel = Channel::from_shared(agent_addr.to_string())
            .map_err(|e| Error::InvalidArgument(format!("bad agent addr: {e}")))?
            .connect()
            .await
            .map_err(|e| Error::Internal(format!("connect failed: {e}")))?;
        let mut client = ForwardAgentServiceClient::new(channel);

        let job_id = Uuid::new_v4().to_string();
        let (tx, rx) = mpsc::channel::<ServerMessage>(8);
        tx.send(ServerMessage {
            kind: Some(server_message::Kind::ExecRequest(ExecRequest {
                job_id,
                command: command.to_string(),
                args: args.to_vec(),
                timeout_secs: None,
                working_dir: None,
            })),
        })
        .await
        .map_err(|_| Error::Internal("outbound channel closed".into()))?;

        let response = client
            .open_forward_channel(ReceiverStream::new(rx))
            .await
            .map_err(|e| Error::Internal(format!("forward channel: {e}")))?;
        let mut inbound = response.into_inner();

        let mut output = String::new();
        let mut exit_code = None;
        while let Some(msg) = inbound
            .message()
            .await
            .map_err(|e| Error::Internal(format!("forward stream: {e}")))?
        {
            if let Some(agent_message::Kind::ExecResult(er)) = msg.kind {
                if let Some(chunk) = er.chunk {
                    output.push_str(&String::from_utf8_lossy(&chunk.data));
                }
                if er.finished {
                    exit_code = er.exit_code;
                    break;
                }
            }
        }

        Ok((output, exit_code))
    }
}
