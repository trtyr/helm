//! 应用层：Agent 二进制现场编译生成器。
//!
//! 按目标平台调用 `cargo build -p helm-agent --release --target <triple>` 现场交叉编译，
//! 编译时通过 HELM_BAKE_* 环境变量把 Server 地址与注册 token 烙入二进制（agent/build.rs
//! 监听变更触发重编译）；agent_id 不烙入，下载后按主机名自动生成——一份二进制通吃同平台主机。

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use tokio::io::AsyncBufReadExt;
use uuid::Uuid;

/// 单条生成任务的状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GenStatus {
    Compiling,
    Ready,
    Failed,
}

impl GenStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            GenStatus::Compiling => "compiling",
            GenStatus::Ready => "ready",
            GenStatus::Failed => "failed",
        }
    }
}

/// 一条生成任务。字段快照经 [`AgentGenService::view`] 序列化给 HTTP 层。
pub struct GenJob {
    pub id: Uuid,
    pub os: String,
    pub arch: String,
    pub triple: String,
    /// 烙入的 Server gRPC 地址（reverse 模式 Agent 连入目标）
    pub server_addr: String,
    /// 烙入的连接模式：reverse | forward
    pub conn_mode: String,
    /// 烙入的监听地址（forward 模式 Agent 对外监听）
    pub listen_addr: String,
    pub status: Mutex<GenStatus>,
    pub error: Mutex<Option<String>>,
    pub file_size: Mutex<Option<u64>>,
    pub created_at: DateTime<Utc>,
    log: Mutex<VecDeque<String>>,
    artifact: Mutex<Option<PathBuf>>,
}

const LOG_CAP: usize = 400;

/// 目标平台 → Rust triple 与产物扩展名。
pub fn triple_for(os: &str, arch: &str) -> Option<(&'static str, &'static str)> {
    match (os, arch) {
        ("windows", "x86_64") => Some(("x86_64-pc-windows-msvc", ".exe")),
        ("windows", "aarch64") => Some(("aarch64-pc-windows-msvc", ".exe")),
        ("linux", "x86_64") => Some(("x86_64-unknown-linux-musl", "")),
        ("linux", "aarch64") => Some(("aarch64-unknown-linux-musl", "")),
        ("macos", "x86_64") => Some(("x86_64-apple-darwin", "")),
        ("macos", "aarch64") => Some(("aarch64-apple-darwin", "")),
        _ => None,
    }
}

/// Agent 生成器：任务注册表 + cargo 编译编排。
#[derive(Clone)]
pub struct AgentGenService {
    inner: Arc<Inner>,
}

struct Inner {
    source_dir: PathBuf,
    jobs: Mutex<HashMap<Uuid, Arc<GenJob>>>,
}

impl AgentGenService {
    pub fn new(source_dir: impl Into<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Inner {
                source_dir: source_dir.into(),
                jobs: Mutex::new(HashMap::new()),
            }),
        }
    }

    /// 创建生成任务并异步开始编译（立即返回，进度经 log/状态轮询）。
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &self,
        os: &str,
        arch: &str,
        conn_mode: &str,
        server_addr: String,
        listen_addr: String,
        token: String,
    ) -> Arc<GenJob> {
        let (triple, _) = triple_for(os, arch).unwrap_or(("unknown", ""));
        let job = Arc::new(GenJob {
            id: Uuid::new_v4(),
            os: os.to_string(),
            arch: arch.to_string(),
            triple: triple.to_string(),
            server_addr,
            conn_mode: conn_mode.to_string(),
            listen_addr,
            status: Mutex::new(GenStatus::Compiling),
            error: Mutex::new(None),
            file_size: Mutex::new(None),
            created_at: Utc::now(),
            log: Mutex::new(VecDeque::new()),
            artifact: Mutex::new(None),
        });
        self.inner.jobs.lock().unwrap().insert(job.id, job.clone());
        self.spawn_build(job.clone(), token);
        job
    }

    pub fn list(&self) -> Vec<Arc<GenJob>> {
        let jobs = self.inner.jobs.lock().unwrap();
        let mut v: Vec<_> = jobs.values().cloned().collect();
        v.sort_by_key(|j| std::cmp::Reverse(j.created_at));
        v
    }

    pub fn get(&self, id: Uuid) -> Option<Arc<GenJob>> {
        self.inner.jobs.lock().unwrap().get(&id).cloned()
    }

    /// 任务视图 + 日志尾部（HTTP 序列化用）。
    pub fn view(job: &GenJob, log_tail: usize) -> serde_json::Value {
        let log = job.log.lock().unwrap();
        let tail: Vec<String> = log.iter().rev().take(log_tail).rev().cloned().collect();
        drop(log);
        serde_json::json!({
            "id": job.id,
            "os": job.os,
            "arch": job.arch,
            "triple": job.triple,
            "server_addr": job.server_addr,
            "conn_mode": job.conn_mode,
            "listen_addr": job.listen_addr,
            "status": job.status.lock().unwrap().as_str(),
            "error": *job.error.lock().unwrap(),
            "file_size": *job.file_size.lock().unwrap(),
            "created_at": job.created_at,
            "log_tail": tail,
        })
    }

    /// 就绪任务的产物路径与下载文件名。
    pub fn artifact(&self, id: Uuid) -> Option<(PathBuf, String)> {
        let job = self.get(id)?;
        if *job.status.lock().unwrap() != GenStatus::Ready {
            return None;
        }
        let path = job.artifact.lock().unwrap().clone()?;
        let (_, ext) = triple_for(&job.os, &job.arch).unwrap_or(("", ""));
        let mode = if job.conn_mode == "forward" {
            "-forward"
        } else {
            ""
        };
        let filename = format!(
            "helm-agent-{os}-{arch}{mode}{ext}",
            os = job.os,
            arch = job.arch
        );
        Some((path, filename))
    }

    fn spawn_build(&self, job: Arc<GenJob>, token: String) {
        let source_dir = self.inner.source_dir.clone();
        tokio::spawn(async move {
            let mut cmd = tokio::process::Command::new("cargo");
            cmd.args([
                "build",
                "-p",
                "helm-agent",
                "--release",
                "--target",
                &job.triple,
            ])
            .current_dir(&source_dir)
            // agent_id 故意不烙入：目标机首跑按主机名自动生成
            .env("HELM_BAKE_SERVER_ADDR", &job.server_addr)
            .env("HELM_BAKE_AGENT_TOKEN", &token)
            .env("HELM_BAKE_CONN_MODE", &job.conn_mode)
            .env("HELM_BAKE_LISTEN_ADDR", &job.listen_addr)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

            // Linux musl 交叉编译：注入 musl 交叉工具链（zig cc 包装脚本）。
            // cc-rs 按 `CC_<target>` 查找 C 编译器，rustc 按 `CARGO_TARGET_<target>_LINKER`
            // 查找链接器，二者均指向 `<triple>-gcc` 包装脚本（文件名即约定）。
            // 工具目录默认 `<源码工作区>/.cargo-musl/bin`，可用 HELM_CROSS_TOOLS_DIR 覆盖。
            if job.triple.ends_with("linux-musl") {
                let tools = std::env::var("HELM_CROSS_TOOLS_DIR").unwrap_or_else(|_| {
                    source_dir
                        .join(".cargo-musl")
                        .join("bin")
                        .to_string_lossy()
                        .into_owned()
                });
                // cc-rs/rustc 会把子进程 cwd 切到构建目录，工具路径必须绝对化
                let tools = std::path::absolute(&tools)
                    .unwrap_or_else(|_| std::path::PathBuf::from(&tools))
                    .to_string_lossy()
                    .into_owned();
                let gcc = format!("{tools}\\x86_64-linux-musl-gcc.cmd");
                let ar = format!("{tools}\\x86_64-linux-musl-ar.cmd");
                if !std::path::Path::new(&gcc).exists() {
                    fail(
                        &job,
                        format!(
                            "缺少 musl 交叉工具链：未找到 {gcc}（需 zig cc 包装脚本，目录可用 HELM_CROSS_TOOLS_DIR 指定）"
                        ),
                    )
                    .await;
                    return;
                }
                let env_suffix = job.triple.replace('-', "_").to_uppercase();
                let path = std::env::var("PATH").unwrap_or_default();
                cmd.env("PATH", format!("{tools};{path}"));
                cmd.env(format!("CC_{env_suffix}"), &gcc);
                cmd.env(format!("CXX_{env_suffix}"), &gcc);
                cmd.env(format!("AR_{env_suffix}"), &ar);
                cmd.env(format!("CARGO_TARGET_{env_suffix}_LINKER"), &gcc);
                // crt 由 zig（链接器驱动）提供：关闭 rustc 自包含 crt，避免 rcrt1.o 与
                // zig 的 crt1.o 重复定义 _start
                cmd.env("CARGO_ENCODED_RUSTFLAGS", "-Clink-self-contained=no");
            }

            let mut child = match cmd.spawn() {
                Ok(c) => c,
                Err(e) => {
                    fail(
                        &job,
                        format!("cargo 启动失败（Server 侧需安装 Rust 工具链）: {e}"),
                    )
                    .await;
                    return;
                }
            };

            // stdout/stderr 并发汇入日志环
            let mut readers = Vec::new();
            if let Some(out) = child.stdout.take() {
                readers.push(tokio::spawn(pump(out, job.clone())));
            }
            if let Some(err) = child.stderr.take() {
                readers.push(tokio::spawn(pump(err, job.clone())));
            }
            let status = child.wait().await;
            for r in readers {
                let _ = r.await;
            }

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
                            push_log(&job, "✓ 编译完成，可下载").await;
                        }
                        Err(e) => {
                            fail(&job, format!("编译成功但未找到产物: {e}")).await;
                        }
                    }
                }
                Ok(s) => {
                    fail(&job, format!("cargo 退出码 {s}（目标平台工具链可能未安装，rustup target list --installed 查看）")).await;
                }
                Err(e) => {
                    fail(&job, format!("cargo 执行异常: {e}")).await;
                }
            }
        });
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

async fn push_log(job: &GenJob, line: &str) {
    let mut log = job.log.lock().unwrap();
    log.push_back(line.to_string());
    while log.len() > LOG_CAP {
        log.pop_front();
    }
}

async fn fail(job: &GenJob, msg: String) {
    push_log(job, &format!("✗ {msg}")).await;
    *job.error.lock().unwrap() = Some(msg);
    *job.status.lock().unwrap() = GenStatus::Failed;
}

/// 解析 Agent 连入地址：监听器 bind 地址中的通配主机部分替换为服务器主内网 IPv4。
pub fn resolve_server_addr(listener_addr: &str) -> String {
    let host_port = listener_addr.trim();
    let (host, port) = match host_port.rsplit_once(':') {
        Some((h, p)) => (h, p),
        None => (host_port, "50051"),
    };
    let host = host.trim_start_matches("[").trim_end_matches("]");
    let effective = match host {
        "" | "0.0.0.0" | "::" => detect_lan_ipv4().unwrap_or_else(|| "127.0.0.1".to_string()),
        other => other.to_string(),
    };
    format!("http://{effective}:{port}")
}

/// 探测服务器主内网 IPv4：首个非回环私网地址（排除 lo/docker/veth 虚拟口）。
fn detect_lan_ipv4() -> Option<String> {
    let nets = sysinfo::Networks::new_with_refreshed_list();
    let mut interfaces: Vec<_> = nets.iter().collect();
    interfaces.sort_by(|a, b| a.0.cmp(b.0));
    for (name, data) in interfaces {
        let lower = name.to_lowercase();
        if lower.starts_with("lo")
            || lower.starts_with("docker")
            || lower.starts_with("veth")
            || lower.starts_with("virbr")
        {
            continue;
        }
        for ip in data.ip_networks() {
            if let std::net::IpAddr::V4(v4) = ip.addr
                && !v4.is_loopback()
                && v4.is_private()
            {
                return Some(v4.to_string());
            }
        }
    }
    None
}

/// 产物路径探测的单测辅助：确认 triple 映射表完整。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triple_mapping_covers_supported_platforms() {
        assert_eq!(
            triple_for("windows", "x86_64"),
            Some(("x86_64-pc-windows-msvc", ".exe"))
        );
        assert_eq!(
            triple_for("linux", "x86_64"),
            Some(("x86_64-unknown-linux-musl", ""))
        );
        assert_eq!(
            triple_for("macos", "aarch64"),
            Some(("aarch64-apple-darwin", ""))
        );
        assert_eq!(triple_for("plan9", "x86_64"), None);
    }

    #[test]
    fn resolve_addr_replaces_wildcard_host() {
        let addr = resolve_server_addr("0.0.0.0:50051");
        assert!(addr.starts_with("http://") && addr.ends_with(":50051"));
        assert_eq!(
            resolve_server_addr("192.168.1.5:50051"),
            "http://192.168.1.5:50051"
        );
    }
}
