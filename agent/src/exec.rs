//! 命令执行：接收 ExecRequest，执行并回传结果。

use anyhow::Result;
use helm_proto::pb::{AgentMessage, ExecResult, StreamChunk, agent_message, stream_chunk};
use tokio::sync::mpsc;

/// 执行命令并回传结果。始终以 finished 结果收尾。
pub async fn run_and_report(
    job_id: &str,
    command: &str,
    args: &[String],
    tx: &mpsc::Sender<AgentMessage>,
) {
    match run_command(command, args).await {
        Ok((stdout, stderr, exit_code)) => {
            if !stdout.is_empty() {
                let _ = tx
                    .send(chunk(job_id, stream_chunk::Kind::Stdout as i32, stdout))
                    .await;
            }
            if !stderr.is_empty() {
                let _ = tx
                    .send(chunk(job_id, stream_chunk::Kind::Stderr as i32, stderr))
                    .await;
            }
            let _ = tx.send(finished(job_id, exit_code, None)).await;
        }
        Err(e) => {
            let _ = tx.send(finished(job_id, None, Some(e.to_string()))).await;
        }
    }
}

async fn run_command(command: &str, args: &[String]) -> Result<(String, String, Option<i32>)> {
    let output = crate::child::quiet_tokio(command)
        .args(args)
        .output()
        .await?;
    let stdout = crate::encoding::decode_console(&output.stdout);
    let stderr = crate::encoding::decode_console(&output.stderr);
    let exit_code = output.status.code();
    Ok((stdout, stderr, exit_code))
}

fn chunk(job_id: &str, kind: i32, data: String) -> AgentMessage {
    AgentMessage {
        kind: Some(agent_message::Kind::ExecResult(ExecResult {
            job_id: job_id.to_string(),
            chunk: Some(StreamChunk {
                kind,
                data: data.into_bytes(),
            }),
            exit_code: None,
            error: None,
            finished: false,
        })),
    }
}

fn finished(job_id: &str, exit_code: Option<i32>, error: Option<String>) -> AgentMessage {
    AgentMessage {
        kind: Some(agent_message::Kind::ExecResult(ExecResult {
            job_id: job_id.to_string(),
            chunk: None,
            exit_code,
            error,
            finished: true,
        })),
    }
}
