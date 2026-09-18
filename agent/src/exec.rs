//! 命令执行：接收 ExecRequest，执行并回传结果。
//! 支持超时（ExecRequest.timeout_secs，超时杀进程并以 timed_out 上报）
//! 与取消（ServerMessage::JobCancel，杀进程并以 cancelled 上报）。

use anyhow::Result;
use helm_proto::pb::{AgentMessage, ExecResult, StreamChunk, agent_message, stream_chunk};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tokio::io::AsyncReadExt;
use tokio::sync::{mpsc, watch};

/// 单流输出分块上限（B6）：超过即截断并置 truncated 标志。
const MAX_STREAM_OUTPUT: usize = 1 << 20; // 1 MiB / 流

/// 子进程退出后等待管道 EOF 的兜底上限：EOF 事件在 Windows 上偶发不传播，
/// 超时后直接取共享缓冲（剩余数据必在缓冲内，不丢输出）。
const DRAIN_TIMEOUT: Duration = Duration::from_secs(3);

/// 输出分块大小（发送粒度）。
const STREAM_CHUNK_SIZE: usize = 64 * 1024;

/// 管道读循环：读到 EOF（或出错）为止，字节持续写入共享缓冲。
async fn read_pipe_into(pipe: &mut (impl tokio::io::AsyncRead + Unpin), buf: Arc<Mutex<Vec<u8>>>) {
    let mut chunk = vec![0u8; 64 * 1024];
    loop {
        match pipe.read(&mut chunk).await {
            Ok(0) => break, // EOF
            Ok(n) => {
                if let Ok(mut b) = buf.lock() {
                    b.extend_from_slice(&chunk[..n]);
                }
            }
            Err(_) => break,
        }
    }
}

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
    tracing::info!(job_id, command, "run_and_report: spawning child process");
    let fut = run_command(job_id, command, args);
    let outcome = match timeout_secs {
        Some(secs) => match tokio::time::timeout(Duration::from_secs(secs as u64), fut).await {
            Ok(result) => Some(result),
            Err(_) => {
                // 超时不计入取消标记
                CANCELLED.lock().unwrap().remove(job_id);
                report_timeout(job_id, secs, tx).await;
                return;
            }
        },
        None => Some(fut.await),
    };

    match outcome {
        Some(Ok((stdout, stderr, exit_code))) => {
            // B6：单流输出超过上限即截断（truncated 标志随最终帧上报）
            let mut truncated =
                send_stream(job_id, stream_chunk::Kind::Stdout as i32, stdout, tx).await;
            if send_stream(job_id, stream_chunk::Kind::Stderr as i32, stderr, tx).await {
                truncated = true;
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
                        false,
                    ))
                    .await;
            } else {
                let _ = tx
                    .send(finish_frame(
                        job_id, exit_code, None, false, false, truncated,
                    ))
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

    // 输出管道分离读取（wait_with_output 会消耗 Child，无法与取消分支共存）。
    // 读循环写入共享缓冲：子进程退出后若管道 EOF 事件不传播（Windows/tokio 偶发），
    // 以 DRAIN_TIMEOUT 兜底取已缓冲数据，不丢输出也不永久卡死。
    let mut stdout_pipe = child.stdout.take().expect("stdout piped");
    let mut stderr_pipe = child.stderr.take().expect("stderr piped");
    let stdout_buf = Arc::new(std::sync::Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(std::sync::Mutex::new(Vec::new()));
    let read_stdout = {
        let buf = stdout_buf.clone();
        tokio::spawn(async move { read_pipe_into(&mut stdout_pipe, buf).await })
    };
    let read_stderr = {
        let buf = stderr_buf.clone();
        tokio::spawn(async move { read_pipe_into(&mut stderr_pipe, buf).await })
    };

    let status = tokio::select! {
        st = child.wait() => {
            let st = st?;
            tracing::info!(job_id, exit_code = st.code(), "child process exited");
            st
        }
        // JobCancel 到达：杀进程后收尾（wait 返回被杀状态）
        _ = cancel_rx.changed() => {
            let _ = child.start_kill();
            let st = child.wait().await?;
            tracing::info!(job_id, "child process killed by cancel");
            st
        }
    };
    CANCEL_TX.lock().unwrap().remove(job_id);
    let job_id_owned = job_id.to_string();
    let stdout_raw = drain_pipe(&stdout_buf, read_stdout, &job_id_owned, "stdout").await;
    let stderr_raw = drain_pipe(&stderr_buf, read_stderr, &job_id_owned, "stderr").await;
    let stdout = crate::encoding::decode_console(&stdout_raw);
    let stderr = crate::encoding::decode_console(&stderr_raw);
    let exit_code = status.code();
    Ok((stdout, stderr, exit_code))
}

/// 等读任务收尾：EOF 正常到达则拿到全量输出；EOF 不传播（Windows/tokio 偶发）
/// 时以 DRAIN_TIMEOUT 兜底——子进程已退出，剩余数据必在缓冲内，直接取走不丢。
async fn drain_pipe(
    buf: &Arc<Mutex<Vec<u8>>>,
    handle: tokio::task::JoinHandle<()>,
    job_id: &str,
    what: &'static str,
) -> Vec<u8> {
    match tokio::time::timeout(DRAIN_TIMEOUT, handle).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            tracing::warn!(job_id, pipe = what, "pipe reader task failed");
        }
        Err(_) => {
            tracing::warn!(
                job_id,
                pipe = what,
                "output pipe EOF not delivered within 3s (using buffered data)"
            );
        }
    }
    buf.lock().unwrap_or_else(|p| p.into_inner()).clone()
}

/// 单流输出分块发送（B6）：每流超过 `MAX_STREAM_OUTPUT` 字节即截断，
/// 超时分支上报：进程已被杀，输出丢弃。
async fn report_timeout(job_id: &str, secs: u32, tx: &mpsc::Sender<AgentMessage>) {
    let _ = tx
        .send(finish_frame(
            job_id,
            None,
            Some(format!("timeout: exceeded {secs}s")),
            false,
            true,
            false,
        ))
        .await;
}

/// 单流输出分块发送（B6）：每流超过 `MAX_STREAM_OUTPUT` 字节即截断，
/// 返回是否发生截断（随最终帧 truncated 标志上报）。
async fn send_stream(
    job_id: &str,
    kind: i32,
    data: String,
    tx: &mpsc::Sender<AgentMessage>,
) -> bool {
    let bytes = data.into_bytes();
    let mut offset = 0usize;
    let mut truncated = false;
    while offset < bytes.len() {
        if offset >= MAX_STREAM_OUTPUT {
            truncated = true;
            break;
        }
        let end = (offset + STREAM_CHUNK_SIZE).min(bytes.len());
        let _ = tx
            .send(chunk_bytes(job_id, kind, bytes[offset..end].to_vec()))
            .await;
        offset = end;
    }
    truncated
}

fn chunk_bytes(job_id: &str, kind: i32, data: Vec<u8>) -> AgentMessage {
    AgentMessage {
        kind: Some(agent_message::Kind::ExecResult(ExecResult {
            job_id: job_id.to_string(),
            chunk: Some(StreamChunk { kind, data }),
            exit_code: None,
            error: None,
            finished: false,
            cancelled: false,
            timed_out: false,
            truncated: false,
        })),
    }
}

fn finish_frame(
    job_id: &str,
    exit_code: Option<i32>,
    error: Option<String>,
    cancelled: bool,
    timed_out: bool,
    truncated: bool,
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
            truncated,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 小输出（< 64KB）分块不越界：B6 回归——此前 min 写错导致
    /// `bytes[offset..65536]` 在小输出上 panic（tokio 吞掉后 job 永远 running）。
    #[tokio::test]
    async fn send_stream_handles_output_smaller_than_chunk() {
        let (tx, mut rx) = mpsc::channel(8);
        let truncated =
            send_stream("j1", stream_chunk::Kind::Stdout as i32, "hello".into(), &tx).await;
        assert!(!truncated);
        let mut collected = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Some(agent_message::Kind::ExecResult(r)) = msg.kind
                && let Some(chunk) = r.chunk
            {
                collected.extend_from_slice(&chunk.data);
            }
        }
        assert_eq!(collected, b"hello");
    }

    #[tokio::test]
    async fn send_stream_exact_chunk_boundary() {
        // 恰好 64KB：单块发出，无截断
        let (tx, mut rx) = mpsc::channel(4);
        let data = "x".repeat(STREAM_CHUNK_SIZE);
        let truncated =
            send_stream("j1", stream_chunk::Kind::Stdout as i32, data.clone(), &tx).await;
        assert!(!truncated);
        let mut total = 0usize;
        while let Ok(msg) = rx.try_recv() {
            if let Some(agent_message::Kind::ExecResult(r)) = msg.kind
                && let Some(chunk) = r.chunk
            {
                total += chunk.data.len();
            }
        }
        assert_eq!(total, STREAM_CHUNK_SIZE);
    }

    #[test]
    fn finish_frame_carries_flags() {
        let msg = finish_frame("j1", Some(0), None, false, false, false);
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert_eq!(r.job_id, "j1");
                assert!(r.finished);
                assert!(!r.cancelled);
                assert!(!r.timed_out);
                assert!(!r.truncated);
            }
            _ => panic!("exec result expected"),
        }
        let msg = finish_frame(
            "j2",
            None,
            Some("cancelled by server".into()),
            true,
            false,
            false,
        );
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert!(r.cancelled);
                assert!(!r.timed_out);
            }
            _ => panic!("exec result expected"),
        }
        let msg = finish_frame("j3", None, Some("timeout".into()), false, true, false);
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert!(!r.cancelled);
                assert!(r.timed_out);
            }
            _ => panic!("exec result expected"),
        }
        let msg = finish_frame("j4", None, None, false, false, true);
        match msg.kind {
            Some(agent_message::Kind::ExecResult(r)) => {
                assert!(r.truncated, "B6: truncation flag must be carried");
            }
            _ => panic!("exec result expected"),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn oversized_output_is_truncated() {
        // B6：输出超过 MAX_STREAM_OUTPUT（1 MiB）→ 停发 chunk + truncated=true
        let (tx, mut rx) = mpsc::channel(256);
        run_and_report(
            "job-big",
            "head",
            &[
                "-c".to_string(),
                "3000000".to_string(),
                "/dev/zero".to_string(),
            ],
            None,
            &tx,
        )
        .await;
        let mut total = 0usize;
        let mut saw_truncated = false;
        while let Ok(msg) = rx.try_recv() {
            if let Some(agent_message::Kind::ExecResult(r)) = msg.kind {
                if let Some(c) = r.chunk {
                    total += c.data.len();
                }
                if r.finished && r.truncated {
                    saw_truncated = true;
                }
            }
        }
        assert!(saw_truncated, "oversized output must set truncated");
        assert!(
            total <= (MAX_STREAM_OUTPUT + STREAM_CHUNK_SIZE) * 2,
            "streamed bytes must be capped near the limit, got {total}"
        );
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
