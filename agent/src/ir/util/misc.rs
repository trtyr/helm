//! 通用小工具：子进程命令构造（平台相关）。
//!
//! 自 `ir/util.rs` 拆出（G7）。原分节标题：「通用」。
//! 平台无关的 `split_csv_line` / `decode_console` 已于 T4 迁往 [`crate::ir::text`]
//! （由 `util` 再导出，调用点零改动）——它们不该被 Windows 门控。

/// 子进程命令（CREATE_NO_WINDOW，不闪控制台）。
pub fn child_cmd(program: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(program);
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
    #[cfg(not(windows))]
    {
        let _ = program; // 平台桩：非 Windows 分支不用 program（改用 cmd），不是吞错
        std::process::Command::new("cmd")
    }
}
