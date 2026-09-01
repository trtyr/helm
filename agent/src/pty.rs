//! PTY 会话管理：spawn 交互 shell + 读写伪终端（跨平台，Windows 走 ConPTY）。

use helm_proto::pb::{AgentMessage, SessionOutput, agent_message};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// 单个 PTY 会话。
struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl Session {
    fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.writer.write_all(data)?;
        self.writer.flush()
    }

    fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()> {
        self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }
}

/// 会话管理器：按 session_id 管理多个会话。
#[derive(Clone, Default)]
pub struct SessionManager {
    inner: Arc<Mutex<HashMap<String, Session>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 打开会话：分配 PTY + 启动 shell，输出经 `tx` 以 SessionOutput 回传。
    pub fn open(
        &self,
        session_id: &str,
        cols: u16,
        rows: u16,
        command: &str,
        tx: mpsc::Sender<AgentMessage>,
    ) -> anyhow::Result<()> {
        let pty_system = native_pty_system();
        let pair = pty_system.openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let child = pair.slave.spawn_command(shell_command(command))?;
        let mut reader = pair.master.try_clone_reader()?;
        let writer = pair.master.take_writer()?;
        let master = pair.master;

        let sid = session_id.to_string();
        // 读线程：阻塞读 master，输出转 SessionOutput 回传。
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let msg = AgentMessage {
                            kind: Some(agent_message::Kind::SessionOutput(SessionOutput {
                                session_id: sid.clone(),
                                data: buf[..n].to_vec(),
                            })),
                        };
                        if tx.blocking_send(msg).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        self.inner.lock().unwrap().insert(
            session_id.to_string(),
            Session {
                master,
                writer,
                child,
            },
        );
        Ok(())
    }

    /// 写入输入到指定会话。
    pub fn input(&self, session_id: &str, data: &[u8]) {
        if let Some(s) = self.inner.lock().unwrap().get_mut(session_id) {
            let _ = s.write(data);
        }
    }

    /// 调整终端窗口大小。
    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) {
        if let Some(s) = self.inner.lock().unwrap().get(session_id) {
            let _ = s.resize(cols, rows);
        }
    }

    /// 关闭会话：杀子进程并移除。
    pub fn close(&self, session_id: &str) {
        if let Some(mut s) = self.inner.lock().unwrap().remove(session_id) {
            let _ = s.child.kill();
        }
    }
}

/// 默认 shell 命令（跨平台）。
fn shell_command(command: &str) -> CommandBuilder {
    if !command.is_empty() {
        return CommandBuilder::new(command);
    }
    #[cfg(target_os = "windows")]
    let prog = "powershell.exe";
    #[cfg(not(target_os = "windows"))]
    let prog = "/bin/sh";
    CommandBuilder::new(prog)
}
