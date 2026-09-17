//! 命令执行：接收 ExecRequest，执行并回传结果。
//! 支持超时（ExecRequest.timeout_secs，超时杀进程并以 timed_out 上报）
//! 与取消（ServerMessage::JobCancel，杀进程并以 cancelled 上报）。

use anyhow::Result;
use helm_proto::pb::{AgentMessage, ExecResult, StreamChunk, agent_message, stream_chunk};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, watch};

/// 运行中 job 的取消通知通道：JobCancel 消息置位，执行循环 select 到后杀进程。
static CANCEL_TX: LazyLock<Mutex<HashMap<String, watch::Sender<bool>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
/// 已请求取消的 job（cancel 与进程自然结束竞态时仍能正确上报 cancelled）。
static CANCELLED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

/// JobCancel 消息入口：标记取消并 kill 运行中子进程（若在跑）。
pub fn request_cancel(job_id: &str) {
    CANCELLED.lock().unwrap().insert(job_id.to_string());
    if let Some(tx) = CANCEL_TX.lock().unwrap().remove(job_id) {
        let _ = tx.send(true);
    }
}

/// 执行命令并回传结果。始终以 finished 结果收尾。
///
/// `timeout_secs`：None = 不限时；Some(n) = 超 n 秒杀进程并以 `timed_out=true` 上报
/// （run_command 的 future 连同 Child（kill_on_drop）被整体 drop，进程随之被杀）。
pub async fn run_and_report(
    job_id: &str,
    command: &str,
    args: &[String],
    timeout_secs: Option<u32>,
    tx: &mpsc::Sender<AgentMessage>,
) {
    let fut = run_command(job_id, command, args);
    let outcome = match timeout_secs {
        Some(secs) => match tokio::time::timeout(Duration::from_secs(secs as u64), fut).await {
            Ok(result) => Some(result),
            Err(_) => {
                // 超时不计入取消标记
                CANCELLED.lock().unwrap().remove(job_id);
                let _ = tx
                    .send(finish_frame(
                        job_id,
                        None,
                        Some(format!("timeout: exceeded {secs}s")),
                        false,
                        true,
                    ))
                    .await;
                return;
            }
        },
        None => Some(fut.await),
    };

    match outcome {
        Some(Ok((stdout, stderr, exit_code))) => {
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
            let cancelled = CANCELLED.lock().unwrap().remove(job_id);
            if cancelled {
                let _ = tx
                    .send(finish_frame(
                        job_id,
                        None,
                        Some("cancelled by server".into()),
                        true,
                        false,
                    ))
                    .await;
            } else {
                let _ = tx
                    .send(finish_frame(job_id, exit_code, None, false, false))
                    .await;
            }
        }
        Some(Err(e)) => {
            let cancelled = CANCELLED.lock().unwrap().remove(job_id);
            if cancelled {
                let _ = tx
                    .send(finish_frame(
                        job_id,
                        None,
                        Some("cancelled by server".into()),
                        true,
                        false,
                    ))
                    .await;
            } else {
                let _ = tx
                    .send(finish_frame(
                        job_id,
                        None,
                        Some(e.to_string()),
                        false,
                        false,
                    ))
                    .await;
            }
        }
        None => unreachable!("timeout branch returns early"),
    }
}

async fn run_command(
    job_id: &str,
    command: &str,
    args: &[String],
) -> Result<(String, String, Option<i32>)> {
    let mut child = crate::child::quiet_tokio(command)
        .args(args)
        .kill_on_drop(true) // 超时路径：future 被整体 drop 时随之杀掉子进程
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let (cancel_tx, mut cancel_rx) = watch::channel(false);
    CANCEL_TX
        .lock()
        .unwrap()
        .insert(job_id.to_string(), cancel_tx);

    // 输出管道分离读取（wait_with_output 会消耗 Child，无法与取消分支共存）
    let mut stdout_pipe = child.stdout.take().expect("stdout piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr piped");
    let read_stdout = tokio::spawn(async move { decode_pipe(&mut stdout_pipe).await });
    let read_stderr = tokio::spawn(async move { decode_pipe(&mut stderr_pipe).await });

    let status = tokio::select! {
        st = child.wait() => st?,
        // JobCancel 到达：杀进程后收尾（wait 返回被杀状态）
        _ = cancel_rx.changed() => {
            let _ = child.start_kill();
            child.wait().await?
        }
    };
    CANCEL_TX.lock().unwrap().remove(job_id);
    let stdout = crate::encoding::decode_console(&read_stdout.await.unwrap_or_default());
    let stderr = crate::encoding::decode_console(&read_stderr.await.unwrap_or_default());
    let exit_code = status.code();
    Ok((stdout, stderr, exit_code))
}

/// 读尽管道原始字节（解码交给 decode_console）。
async fn decode_pipe(pipe: &mut (impl tokio::io::AsyncRead + Unpin)) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = pipe.read_to_end(&mut buf).await;
    buf
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
            cancelled: false,
            timed_out: false,
        })),
    }
}

fn finish_frame(
    job_id: &str,
    exit_code: Option<i32>,
    error: Option<String>,
    cancelled: bool,
    timed_out: bool,
) -> AgentMessage {
    AgentMessage {
        kind: Some(agent_message::Kind::ExecResult(ExecResult {
            job_id: job_id.to_string(),
            chunk: None,
            exit_code,
            error,
            finished: true,
            cancelled,
            timed_out,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finish_frame_carries_flags() {
        let msg = finish_frame("j1", Some(0), None, false, false);
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert_eq!(r.job_id, "j1");
                assert!(r.finished);
                assert!(!r.cancelled);
                assert!(!r.timed_out);
            }
            _ => panic!("exec result expected"),
        }
        let msg = finish_frame("j2", None, Some("cancelled by server".into()), true, false);
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert!(r.cancelled);
                assert!(!r.timed_out);
            }
            _ => panic!("exec result expected"),
        }
        let msg = finish_frame("j3", None, Some("timeout".into()), false, true);
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert!(!r.cancelled);
                assert!(r.timed_out);
            }
            _ => panic!("exec result expected"),
        }
    }

    #[test]
    fn request_cancel_marks_and_is_idempotent() {
        request_cancel("job-x");
        request_cancel("job-x"); // 幂等：重复取消不 panic
        assert!(CANCELLED.lock().unwrap().contains("job-x"));
        // 未在运行的 job：CANCEL_TX 无条目，仅标记（供竞态上报）
        CANCELLED.lock().unwrap().remove("job-x");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_kills_process_and_reports_timed_out() {
        let (tx, mut rx) = mpsc::channel(8);
        let started = std::time::Instant::now();
        run_and_report("job-t", "sleep", &["30".to_string()], Some(1), &tx).await;
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "timeout should return promptly, took {:?}",
            started.elapsed()
        );
        let mut saw_timed_out = false;
        while let Ok(msg) = rx.try_recv() {
            if let Some(agent_message::Kind::ExecResult(r)) = msg.kind
                && r.finished
                && r.timed_out
            {
                saw_timed_out = true;
            }
        }
        assert!(saw_timed_out, "final frame must carry timed_out=true");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn cancel_kills_running_process_and_reports_cancelled() {
        let (tx, mut rx) = mpsc::channel(8);
        let args = vec!["30".to_string()];
        let tx_c = tx.clone();
        let handle = tokio::spawn(async move {
            run_and_report("job-c", "sleep", &args, None, &tx_c).await;
        });
        // 等子进程登记进取消表
        for _ in 0..100 {
            if CANCEL_TX.lock().unwrap().contains_key("job-c") {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        request_cancel("job-c");
        handle.await.unwrap();
        let mut saw_cancelled = false;
        while let Ok(msg) = rx.try_recv() {
            if let Some(agent_message::Kind::ExecResult(r)) = msg.kind
                && r.finished
                && r.cancelled
            {
                saw_cancelled = true;
            }
        }
        assert!(saw_cancelled, "final frame must carry cancelled=true");
    }
}
