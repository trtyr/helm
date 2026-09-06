//! 常驻服务托管：spawn 长跑进程 + 状态/日志上报 + 崩溃重启。

use helm_proto::pb::{AgentMessage, ServiceStatus, agent_message};
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::sync::{mpsc, oneshot};

/// 托管服务句柄：停止信号。
struct Managed {
    kill_tx: oneshot::Sender<()>,
}

/// 服务管理器：按 service_id 管理托管进程。
#[derive(Clone, Default)]
pub struct ServiceManager {
    inner: Arc<Mutex<HashMap<String, Managed>>>,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动服务：spawn 进程 + monitor，状态/日志经 `tx` 上报。
    pub async fn start(
        &self,
        service_id: &str,
        command: &str,
        args: &[String],
        restart_policy: &str,
        tx: mpsc::Sender<AgentMessage>,
    ) {
        self.stop(service_id).await;

        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        self.inner
            .lock()
            .unwrap()
            .insert(service_id.to_string(), Managed { kill_tx });

        let sid = service_id.to_string();
        let rp = restart_policy.to_string();
        let cmd = command.to_string();
        let a = args.to_vec();
        tokio::spawn(async move {
            monitor(sid, kill_rx, rp, cmd, a, tx).await;
        });
    }

    /// 停止服务（杀进程）。
    pub async fn stop(&self, service_id: &str) {
        if let Some(m) = self.inner.lock().unwrap().remove(service_id) {
            let _ = m.kill_tx.send(());
        }
    }
}

/// 监控循环：spawn → 读输出 → 等退出/停止 → （可选）重启。
async fn monitor(
    service_id: String,
    mut kill_rx: oneshot::Receiver<()>,
    restart_policy: String,
    command: String,
    args: Vec<String>,
    tx: mpsc::Sender<AgentMessage>,
) {
    loop {
        let mut child = match crate::child::quiet_tokio(&command)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = send(
                    &tx,
                    &service_id,
                    "failed",
                    None,
                    None,
                    e.to_string().as_bytes(),
                )
                .await;
                return;
            }
        };
        let pid = child.id().map(|p| p as i32);
        let _ = send(&tx, &service_id, "running", pid, None, b"").await;

        spawn_reader(child.stdout.take(), tx.clone(), service_id.clone(), pid);
        spawn_reader(child.stderr.take(), tx.clone(), service_id.clone(), pid);

        let exit_code = tokio::select! {
            _ = &mut kill_rx => {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = send(&tx, &service_id, "exited", pid, None, b"").await;
                return;
            }
            status = child.wait() => {
                status.ok().and_then(|s| s.code())
            }
        };

        let _ = send(
            &tx,
            &service_id,
            terminal_status(exit_code),
            pid,
            exit_code,
            b"",
        )
        .await;

        if !should_restart(&restart_policy) {
            return;
        }
    }
}

/// 服务退出后是否重启（纯函数）。
pub fn should_restart(restart_policy: &str) -> bool {
    restart_policy == "always"
}

/// 由退出码判断服务终态（纯函数）。
pub fn terminal_status(exit_code: Option<i32>) -> &'static str {
    if exit_code == Some(0) {
        "exited"
    } else {
        "failed"
    }
}

/// 读进程输出流，增量上报为 ServiceStatus（status=running 的日志块）。
fn spawn_reader(
    stream: Option<impl tokio::io::AsyncRead + Unpin + Send + 'static>,
    tx: mpsc::Sender<AgentMessage>,
    service_id: String,
    pid: Option<i32>,
) {
    if let Some(mut s) = stream {
        tokio::spawn(async move {
            use tokio::io::AsyncReadExt;
            let mut buf = vec![0u8; 4096];
            loop {
                match s.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if send(&tx, &service_id, "running", pid, None, &buf[..n])
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });
    }
}

/// 上报服务状态/日志。
async fn send(
    tx: &mpsc::Sender<AgentMessage>,
    service_id: &str,
    status: &str,
    pid: Option<i32>,
    exit_code: Option<i32>,
    log: &[u8],
) -> Result<(), ()> {
    let msg = AgentMessage {
        kind: Some(agent_message::Kind::ServiceStatus(ServiceStatus {
            service_id: service_id.to_string(),
            status: status.to_string(),
            pid,
            exit_code,
            log: log.to_vec(),
        })),
    };
    tx.send(msg).await.map_err(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_restart_only_when_always() {
        assert!(should_restart("always"));
        assert!(!should_restart("no"));
        assert!(!should_restart(""));
    }

    #[test]
    fn terminal_status_by_exit_code() {
        assert_eq!(terminal_status(Some(0)), "exited");
        assert_eq!(terminal_status(Some(1)), "failed");
        assert_eq!(terminal_status(None), "failed");
    }
}
