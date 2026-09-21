//! PTY 会话管理：spawn 交互 shell + 读写伪终端（跨平台，Windows 走 ConPTY）。

use helm_proto::pb::{AgentMessage, SessionClosed, SessionOutput, agent_message};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

/// 单个 PTY 会话。
struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
}

impl Session {
    fn write(&mut self, data: &[u8]) -> std::io::Result<()> {
        let mut w = self.writer.lock().unwrap();
        w.write_all(data)?;
        w.flush()
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
        let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let master = pair.master;

        let sid = session_id.to_string();
        let inner = self.inner.clone();
        let sid_for_close = session_id.to_string();
        let writer_for_reader = writer.clone();
        // 读线程：阻塞读 master，输出转 SessionOutput 回传。
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // 终端职责：应答 DSR-CPR（ESC[6n 光标位置查询）。
                        // 不应答时 ConPTY 的首屏初始化会永远等待，表现为终端黑屏无回显。
                        if buf[..n].windows(4).any(|w| w == b"\x1b[6n") {
                            let mut w = writer_for_reader.lock().unwrap();
                            // DSR-CPR 应答：写失败即主端已断，会话即将结束，无需上报
                            let _ = w.write_all(b"\x1b[1;1R");
                            let _ = w.flush();
                        }
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
            // shell 自行退出：发 SessionClosed 并回收会话（双向关闭）。
            let close = AgentMessage {
                kind: Some(agent_message::Kind::SessionClosed(SessionClosed {
                    session_id: sid_for_close.clone(),
                    exit_code: None,
                })),
            };
            let _ = tx.blocking_send(close); // 上报通道已断：SessionClosed 丢失后由 server 侧 session 回收兜底
            inner.lock().unwrap().remove(&sid_for_close);
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
            let _ = s.write(data); // 会话已结束（PTY 关闭）则写入失败，前端会收到 SessionClosed
        }
    }

    /// 调整终端窗口大小。
    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) {
        if let Some(s) = self.inner.lock().unwrap().get(session_id) {
            let _ = s.resize(cols, rows); // 会话已结束则改窗失败，无副作用
        }
    }

    /// 关闭会话：杀子进程并移除。
    pub fn close(&self, session_id: &str) {
        if let Some(mut s) = self.inner.lock().unwrap().remove(session_id) {
            let _ = s.child.kill(); // 尽力杀 PTY 子进程：失败即已自行退出
        }
    }
}

/// 默认 shell 命令（跨平台）。
fn shell_command(command: &str) -> CommandBuilder {
    if !command.is_empty() {
        return CommandBuilder::new(command);
    }
    CommandBuilder::new(default_shell())
}

/// 默认 shell（跨平台，纯函数便于测试）。
pub fn default_shell() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "powershell.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "/bin/sh"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_shell_is_expected() {
        #[cfg(target_os = "windows")]
        assert_eq!(default_shell(), "powershell.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(default_shell(), "/bin/sh");
    }

    #[test]
    fn manager_input_close_unknown_session_is_noop() {
        let m = SessionManager::new();
        m.input("no-such-session", b"ls\r");
        m.resize("no-such-session", 80, 24);
        m.close("no-such-session"); // 不应 panic
    }
}

#[cfg(all(test, windows))]
mod conpty_tests {
    use super::*;

    /// conpty 回环：open → 写 echo → 输出应包含命令回显与结果。
    /// ConPTY 回环：open → 等 DSR-CPR 应答 → 写 echo → 输出应含结果。
    #[test]
    fn conpty_input_output_roundtrip() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentMessage>(1024);

        let mgr = SessionManager::new();
        mgr.open("s1", 80, 24, "", tx).unwrap();

        // 等 conpty 初始化输出（prompt / 光标查询）
        let mut seen = String::new();
        for _ in 0..30 {
            if let Ok(msg) = rx.try_recv()
                && let Some(agent_message::Kind::SessionOutput(o)) = msg.kind
            {
                seen.push_str(&String::from_utf8_lossy(&o.data));
                if seen.contains('>') {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        eprintln!("初始化输出: {seen:?}");

        mgr.input("s1", b"echo CONPTY-OK-12345\r");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while std::time::Instant::now() < deadline {
            if let Ok(msg) = rx.try_recv()
                && let Some(agent_message::Kind::SessionOutput(o)) = msg.kind
            {
                seen.push_str(&String::from_utf8_lossy(&o.data));
                if seen.contains("CONPTY-OK-12345") {
                    break;
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let tail: String = seen
            .chars()
            .rev()
            .take(120)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        eprintln!("最终输出尾部: {tail:?}");
        mgr.close("s1");
        assert!(seen.contains("CONPTY-OK-12345"), "conpty 未回显 echo 输出");
    }
}
