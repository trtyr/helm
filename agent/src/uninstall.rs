//! 自杀卸载：Agent 收到卸载指令后，停止、删除自身二进制、移除服务/自启。

/// 执行自杀卸载：移除自启 → 删除二进制 → 退出进程。
///
/// 由 Server 下发的 `SelfDestruct` 指令触发。永不返回。
pub fn self_destruct(remove_binary: bool) -> ! {
    tracing::warn!(
        remove_binary,
        "uninstall command received, self-destructing"
    );

    remove_autostart();

    if remove_binary {
        remove_binary_file();
    }

    tracing::info!("self-destruct complete, exiting");
    std::process::exit(0);
}

/// 移除服务/自启（best-effort，失败不阻断）。
///
/// 任务/服务名当前为固定默认值，待 Phase 7 服务化落地后改为可配置。
fn remove_autostart() {
    #[cfg(target_os = "windows")]
    {
        // 删除计划任务（与部署时使用的任务名一致）。自毁路径：失败即任务本就不存在，无副作用
        let _ = crate::child::quiet("schtasks")
            .args(["/delete", "/f", "/tn", "helmagent"])
            .output();
    }
    #[cfg(target_os = "linux")]
    {
        // 同上：自毁前停止 systemd 单元，失败即未安装
        let _ = crate::child::quiet("systemctl")
            .args(["disable", "--now", "helm-agent"])
            .output();
    }
    #[cfg(target_os = "macos")]
    {
        // 无自启机制，无需处理。
    }
}

/// 删除自身二进制文件。
///
/// Windows 下运行中的 exe 无法直接删除，需派发延迟删除脚本；
/// Unix（Linux/macOS）下运行中的二进制可直接 unlink。
fn remove_binary_file() {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(error = %e, "failed to resolve current exe path");
            return;
        }
    };

    #[cfg(target_os = "windows")]
    {
        // ping 作为 sleep，等待进程退出后再删除。
        let script = format!("ping -n 3 127.0.0.1 >nul & del /f /q \"{}\"", exe.display());
        match crate::child::quiet("cmd").args(["/c", &script]).spawn() {
            Ok(_) => tracing::info!(path = %exe.display(), "scheduled delayed delete"),
            Err(e) => tracing::warn!(error = %e, "failed to schedule delayed delete"),
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        match std::fs::remove_file(&exe) {
            Ok(()) => tracing::info!(path = %exe.display(), "binary removed"),
            Err(e) => tracing::warn!(path = %exe.display(), error = %e, "failed to remove binary"),
        }
    }
}
