//! ConPTY 最小探针：绕开 SessionManager/channel，直接驱动 portable-pty，
//! 分步打印输入写入后的原始输出，定位 Windows 终端无回显的卡点。

use portable_pty::{CommandBuilder, native_pty_system};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn main() {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(portable_pty::PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
        .expect("openpty");
    let mut child = pair
        .slave
        .spawn_command(CommandBuilder::new("cmd"))
        .expect("spawn cmd");
    let mut reader = pair.master.try_clone_reader().expect("clone reader");
    let writer: Arc<Mutex<Box<dyn Write + Send>>> =
        Arc::new(Mutex::new(pair.master.take_writer().expect("take writer")));

    // 输出泵：原始字节打印（hex 摘要 + 可读文本）
    let writer_for_thread = writer.clone();
    let printer = std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut acc = Vec::new();
        loop {
            match reader.read(&mut buf) {
                Ok(0) => {
                    eprintln!("[probe] reader EOF");
                    break;
                }
                Ok(n) => {
                    acc.extend_from_slice(&buf[..n]);
                    eprintln!(
                        "[probe] read {n}B: {:?}",
                        String::from_utf8_lossy(&buf[..n])
                    );
                    // DSR-CPR：终端应回复光标位置（实验 B：自动应答）
                    if buf[..n].windows(4).any(|w| w == b"[6n") {
                        eprintln!("[probe] replying CPR");
                        let mut w = writer_for_thread.lock().unwrap();
                        let _ = w.write_all(b"[1;1R");
                        let _ = w.flush();
                    }
                }
                Err(e) => {
                    eprintln!("[probe] read err: {e}");
                    break;
                }
            }
        }
        eprintln!("[probe] total {}B", acc.len());
    });

    std::thread::sleep(Duration::from_secs(1));
    eprintln!("[probe] writing echo command...");
    writer
        .lock()
        .unwrap()
        .write_all(b"echo PROBE-OK-12345\r")
        .expect("write");
    writer.lock().unwrap().flush().expect("flush");
    eprintln!("[probe] written at {:?}", Instant::now());

    let deadline = Instant::now() + Duration::from_secs(6);
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(200));
    }
    let status = child.wait();
    eprintln!("[probe] cmd exit: {:?}", status.map(|s| s.exit_code()));
    let _ = printer.join();
    eprintln!("[probe] done");
}
