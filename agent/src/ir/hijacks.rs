//! 映像劫持与登录链劫持（Autoruns "Image Hijacks" 思路）：
//! IFEO Debugger / SilentProcessExit、AppInit_DLLs、Winlogon Shell/Userinit/GinaDLL 异常。
//! 正常系统应零发现——任何一条都是高价值 IR 信号。

use super::util::{Scanner, extract_exe, hkey_local, reg_get_value, reg_subkeys, reg_values, resolve_pe_path};
use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

pub fn scan(sc: &mut Scanner) {
    ifeo(sc);
    appinit(sc);
    winlogon_anomalies(sc);
}

/// IFEO：Debugger 值 = 劫持任意 exe 启动；GlobalFlag 0x200 + SilentProcessExit = 静默退出监控。
fn ifeo(sc: &mut Scanner) {
    const ROOTS: &[&str] = &[
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Image File Execution Options",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Image File Execution Options",
    ];
    for root in ROOTS {
        let label = if root.contains("WOW6432Node") { "IFEO(32)" } else { "IFEO" };
        for sub in reg_subkeys(hkey_local(), root) {
            let full = format!("{root}\\{sub}");
            if let Some(dbg) = reg_get_value(hkey_local(), &full, "Debugger")
                && !dbg.trim().is_empty()
            {
                let exe = extract_exe(&dbg);
                sc.push(
                    "映像劫持",
                    &sub,
                    format!("[{label} Debugger] {dbg}"),
                    "warn",
                    exe,
                    None,
                );
            }
            // 静默进程退出监控（Silent Process Exit 持久化）
            let flags = reg_get_value(hkey_local(), &full, "GlobalFlag")
                .and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())
                .or_else(|| {
                    reg_get_value(hkey_local(), &full, "GlobalFlag")
                        .and_then(|v| v.trim().parse::<u32>().ok())
                });
            if flags == Some(0x200) {
                let mon = format!("{full}\\SilentProcessExit");
                if let Some(m) = reg_get_value(hkey_local(), &mon, "MonitorProcess")
                    && !m.trim().is_empty()
                {
                    sc.push(
                        "映像劫持",
                        &sub,
                        format!("[静默退出监控] MonitorProcess = {m}"),
                        "warn",
                        extract_exe(&m),
                        None,
                    );
                }
            }
        }
    }
}

/// AppInit_DLLs：全局 DLL 注入（HKLM 64/32 + HKCU）。
fn appinit(sc: &mut Scanner) {
    const ROOTS: &[(&str, &str)] = &[
        (
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows",
            "AppInit_DLLs(64)",
        ),
        (
            r"SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Windows",
            "AppInit_DLLs(32)",
        ),
    ];
    for (root, label) in ROOTS {
        if let Some(v) = reg_get_value(hkey_local(), root, "AppInit_DLLs")
            && !v.trim().is_empty()
        {
            sc.push("映像劫持", label, format!("全局 DLL 注入: {v}"), "critical", None, None);
        }
    }
    if let Some(v) = reg_get_value(
        HKEY_CURRENT_USER,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Windows",
        "AppInit_DLLs",
    ) && !v.trim().is_empty()
    {
        sc.push("映像劫持", "AppInit_DLLs(HKCU)", format!("全局 DLL 注入: {v}"), "critical", None, None);
    }
}

/// Winlogon 登录链异常：Shell / Userinit / GinaDLL / Taskman 非默认值。
fn winlogon_anomalies(sc: &mut Scanner) {
    const ROOTS: &[(&str, &str)] = &[
        (
            r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon",
            "Winlogon",
        ),
        (
            r"SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Winlogon",
            "Winlogon(32)",
        ),
    ];
    for (root, label) in ROOTS {
        for (name, val) in reg_values(hkey_local(), root) {
            let anomaly = match name.as_str() {
                // 默认 "explorer.exe"
                "Shell" => !val.eq_ignore_ascii_case("explorer.exe"),
                // 默认 "C:\Windows\system32\userinit.exe,"（可带其他项）
                "Userinit" => {
                    let v = val.to_ascii_lowercase();
                    !v.contains("userinit.exe")
                }
                // 已废弃的 GINA，存在即异常
                "GinaDLL" => true,
                // 默认空/taskman.exe
                "Taskman" => !val.trim().is_empty() && !val.eq_ignore_ascii_case("taskman.exe"),
                // AppSetup 值列表非空时关注
                "AppSetup" => !val.trim().is_empty(),
                _ => false,
            };
            if anomaly {
                let sev = match name.as_str() {
                    "Shell" | "Userinit" | "GinaDLL" => "critical",
                    _ => "warn",
                };
                sc.push(
                    "映像劫持",
                    &format!("{label}\\{name}"),
                    format!("登录链劫持: [{name}] {val}"),
                    sev,
                    None,
                    None,
                );
            }
        }
        // Winlogon\Notify（XP 时代 DLL 通知，现代系统存在即可疑）
        let notify = format!("{root}\\Notify");
        for sub in reg_subkeys(hkey_local(), &notify) {
            if let Some(dll) = reg_get_value(hkey_local(), &format!("{notify}\\{sub}"), "DLLName")
                && !dll.trim().is_empty()
            {
                let path = resolve_pe_path(&dll);
                sc.push(
                    "映像劫持",
                    &format!("Notify\\{sub}"),
                    format!("[Winlogon Notify] {dll}"),
                    "warn",
                    Some(path),
                    None,
                );
            }
        }
    }
}
