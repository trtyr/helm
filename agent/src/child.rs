//! 子进程创建：Agent 是 GUI 子系统程序（Windows 下无控制台可继承），
//! 直接 spawn 控制台程序（powershell/netstat/sc/taskkill...）时系统会新建
//! 可见的控制台窗口，造成周期性黑框闪烁。统一在此构建带 CREATE_NO_WINDOW
//! 标志的静默命令；所有非交互子进程都应走这里。

#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command as StdCommand;

/// 新进程不创建控制台窗口（Windows）。
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// 同步版：构建静默子进程命令。
pub fn quiet(program: &str) -> StdCommand {
    #[allow(unused_mut)]
    let mut cmd = StdCommand::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// 异步版（tokio）：构建静默子进程命令。
pub fn quiet_tokio(program: &str) -> tokio::process::Command {
    #[allow(unused_mut)]
    let mut cmd = tokio::process::Command::new(program);
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}
