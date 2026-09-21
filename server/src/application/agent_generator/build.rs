//! Agent 编译执行的机制层（文件体积拆分，2026-09-21）——自 `agent_generator.rs` 拆出：
//! cargo 命令构造、输出泵、结束判定，以及日志/失败写入的共享助手。
//!
//! 业务编排（任务生命周期、日志环持有、目标三元组解析）留在父模块；本模块只关心
//! 「怎么起一次编译、怎么收集输出、怎么判定结果」。

use super::{GenJob, GenStatus, LOG_CAP, triple_for};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

/// 构造 cargo build 命令：目标三元组、工作目录、烙入的环境变量、输出管道。
pub(super) fn cargo_build_command(
    source_dir: &std::path::Path,
    job: &GenJob,
    token: &str,
) -> tokio::process::Command {
    let mut cmd = tokio::process::Command::new("cargo");
    cmd.args([
        "build",
        "-p",
        "helm-agent",
        "--release",
        "--target",
        &job.triple,
    ])
    .current_dir(source_dir)
    // agent_id 故意不烙入：目标机首跑按主机名自动生成
    .env("HELM_BAKE_SERVER_ADDR", &job.server_addr)
    .env("HELM_BAKE_AGENT_TOKEN", token)
    .env("HELM_BAKE_CONN_MODE", &job.conn_mode)
    .env("HELM_BAKE_LISTEN_ADDR", &job.listen_addr)
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .kill_on_drop(true);
    cmd
}

/// 把子进程 stdout/stderr 并发汇入日志环，返回读取任务句柄。
pub(super) fn spawn_output_pumps(
    child: &mut tokio::process::Child,
    job: &Arc<GenJob>,
) -> Vec<tokio::task::JoinHandle<()>> {
    let mut readers = Vec::new();
    if let Some(out) = child.stdout.take() {
        readers.push(tokio::spawn(pump(out, job.clone())));
    }
    if let Some(err) = child.stderr.take() {
        readers.push(tokio::spawn(pump(err, job.clone())));
    }
    readers
}

/// 编译结束的状态判定：成功则登记产物（产物缺失也算失败），失败/异常给出可操作提示。
pub(super) async fn finish_build(
    source_dir: &std::path::Path,
    job: &Arc<GenJob>,
    status: std::io::Result<std::process::ExitStatus>,
) {
    match status {
        Ok(s) if s.success() => {
            let artifact = source_dir
                .join("target")
                .join(&job.triple)
                .join("release")
                .join(format!("helm-agent{}", triple_ext(&job.os, &job.arch)));
            match tokio::fs::metadata(&artifact).await {
                Ok(meta) => {
                    *job.file_size.lock().unwrap() = Some(meta.len());
                    *job.artifact.lock().unwrap() = Some(artifact);
                    *job.status.lock().unwrap() = GenStatus::Ready;
                    push_log(job, "✓ 编译完成，可下载").await;
                }
                Err(e) => {
                    fail(job, format!("编译成功但未找到产物: {e}")).await;
                }
            }
        }
        Ok(s) => {
            fail(job, format!("cargo 退出码 {s}（目标平台工具链可能未安装，rustup target list --installed 查看）")).await;
        }
        Err(e) => {
            fail(job, format!("cargo 执行异常: {e}")).await;
        }
    }
}

fn triple_ext(os: &str, arch: &str) -> &'static str {
    triple_for(os, arch).map(|(_, ext)| ext).unwrap_or("")
}

/// 逐行把编译输出写入任务日志环（容量上限，旧的丢弃）。
async fn pump<R: tokio::io::AsyncRead + Unpin>(reader: R, job: Arc<GenJob>) {
    let mut lines = tokio::io::BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        push_log(&job, &line).await;
    }
}

pub(super) async fn push_log(job: &GenJob, line: &str) {
    let mut log = job.log.lock().unwrap();
    log.push_back(line.to_string());
    while log.len() > LOG_CAP {
        log.pop_front();
    }
}

pub(super) async fn fail(job: &GenJob, msg: String) {
    push_log(job, &format!("✗ {msg}")).await;
    *job.error.lock().unwrap() = Some(msg);
    *job.status.lock().unwrap() = GenStatus::Failed;
}
