//! 登录与外壳类持久化（Autoruns "Logon / Explorer" 标签页）：
//! Run/RunOnce（含其他用户 + AutorunsDisabled 禁用区）、Winlogon GPExtensions、
//! Active Setup、启动文件夹、全量外壳扩展（各类 shellex 处理器 + 服务对象 + 执行钩子）。

use super::util::{
    AUTORUNS_DISABLED_SUBKEY, Entry, Scanner, extract_exe, hkey_local, reg_get_value, reg_subkeys,
    reg_values, resolve_pe_path,
};
use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, HKEY_USERS};

mod shell;

use shell::{explorer_shell, startup_folders};

pub fn scan(sc: &mut Scanner) {
    logon_run(sc);
    logon_active_setup(sc);
    logon_gpextensions(sc);
    startup_folders(sc);
    explorer_shell(sc);
}

// ---------------------------------------------------------------------------
// 登录：Run / RunOnce / RunOnceEx / 策略 Run（每用户 HKU + HKCU + HKLM）
// ---------------------------------------------------------------------------

const RUN_KEYS: &[(&str, &str)] = &[
    (r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "HKLM"),
    (r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce", "HKLM"),
    (
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnceEx",
        "HKLM",
    ),
    (
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run",
        "HKLM",
    ),
    (
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce",
        "HKLM",
    ),
    (
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer\Run",
        "HKLM 策略",
    ),
];

fn logon_run(sc: &mut Scanner) {
    for (path, label) in RUN_KEYS {
        scan_run_key(sc, hkey_local(), "HKLM", label, path);
    }
    scan_run_key(
        sc,
        HKEY_CURRENT_USER,
        "HKCU",
        "HKCU",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
    );
    scan_run_key(
        sc,
        HKEY_CURRENT_USER,
        "HKCU",
        "HKCU",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
    );
    scan_run_key(
        sc,
        HKEY_CURRENT_USER,
        "HKCU",
        "HKCU 策略",
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\Explorer\Run",
    );
    // 其他已加载用户配置文件的 Run 键（HKEY_USERS 枚举）
    other_users_run(sc);
}

/// 扫描一个 Run 键（含 AutorunsDisabled 禁用区）。
fn scan_run_key(sc: &mut Scanner, hive: HKEY, hive_tag: &str, label: &str, path: &str) {
    for (name, val) in reg_values(hive, path) {
        let exe = extract_exe(&val);
        sc.push_entry(
            Entry::new(
                "登录",
                &name,
                format!("[{label}\\{path}] {val}"),
                "info",
                exe,
            )
            .op_key(format!("reg\u{1f}{hive_tag}\u{1f}{path}\u{1f}{name}")),
        );
    }
    // 禁用区
    let disabled_path = format!("{path}\\{AUTORUNS_DISABLED_SUBKEY}");
    for (name, val) in reg_values(hive, &disabled_path) {
        let exe = extract_exe(&val);
        sc.push_entry(
            Entry::new(
                "登录",
                &name,
                format!("[{label}\\{disabled_path}] {val}"),
                "info",
                exe,
            )
            .op_key(format!(
                "reg\u{1f}{hive_tag}\u{1f}{disabled_path}\u{1f}{name}"
            ))
            .disabled(),
        );
    }
}

/// 其他用户配置文件（HKEY_USERS 已加载的 hive；当前用户已由 HKCU 扫过，跳过）。
fn other_users_run(sc: &mut Scanner) {
    let current = current_user_sid();
    for sid in reg_subkeys(HKEY_USERS, "") {
        if sid.ends_with("_Classes") || sid == ".DEFAULT" {
            continue;
        }
        if let Some(cur) = &current
            && sid == *cur
        {
            continue;
        }
        let user = sid_to_username(&sid).unwrap_or_else(|| sid.clone());
        for (rel, label) in [
            (r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run", "Run"),
            (
                r"SOFTWARE\Microsoft\Windows\CurrentVersion\RunOnce",
                "RunOnce",
            ),
        ] {
            let full = format!("{sid}\\{rel}");
            for (name, val) in reg_values(HKEY_USERS, &full) {
                let exe = extract_exe(&val);
                sc.push_entry(
                    Entry::new(
                        "登录",
                        &name,
                        format!("[HKU\\{user}\\{label}] {val}"),
                        "info",
                        exe,
                    )
                    .op_key(format!("reg\u{1f}HKU:{sid}\u{1f}{rel}\u{1f}{name}"))
                    .desc(format!("用户: {user}")),
                );
            }
        }
    }
}

/// SID → 用户名（读 ProfileList 的 ProfileImagePath 末段）。
fn sid_to_username(sid: &str) -> Option<String> {
    let path = format!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList\{sid}");
    let img = reg_get_value(hkey_local(), &path, "ProfileImagePath")?;
    let name = img.rsplit('\\').next()?.to_string();
    (!name.is_empty()).then_some(name)
}

/// 当前进程用户的 SID（用 USERPROFILE 与 ProfileList 匹配）。
fn current_user_sid() -> Option<String> {
    let profile = std::env::var("USERPROFILE").ok()?;
    let root = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\ProfileList";
    for sid in reg_subkeys(hkey_local(), root) {
        if let Some(img) =
            reg_get_value(hkey_local(), &format!("{root}\\{sid}"), "ProfileImagePath")
            && std::path::Path::new(&img) == std::path::Path::new(&profile)
        {
            return Some(sid);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 登录：Active Setup（StubPath——装机后每用户触发一次，常被滥用）
// ---------------------------------------------------------------------------

fn logon_active_setup(sc: &mut Scanner) {
    const ROOTS: &[&str] = &[
        r"SOFTWARE\Microsoft\Active Setup\Installed Components",
        r"SOFTWARE\WOW6432Node\Microsoft\Active Setup\Installed Components",
    ];
    for root in ROOTS {
        for sub in reg_subkeys(hkey_local(), root) {
            let full = format!("{root}\\{sub}");
            let stub = reg_get_value(hkey_local(), &full, "StubPath").unwrap_or_default();
            if stub.trim().is_empty() {
                continue;
            }
            let title = reg_get_value(hkey_local(), &full, "")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(sub.clone());
            let exe = extract_exe(&stub);
            sc.push_entry(
                Entry::new(
                    "登录",
                    &title,
                    format!("[Active Setup] {stub}"),
                    "info",
                    exe,
                )
                .op_key(format!("reg\u{1f}HKLM\u{1f}{full}\u{1f}StubPath"))
                .desc(sub),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 登录：Winlogon 组策略扩展（每用户/每次登录加载的 DLL）
// ---------------------------------------------------------------------------

const GPEXT_ROOTS: &[&str] = &[
    r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\GPExtensions",
    r"SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Winlogon\GPExtensions",
];

fn logon_gpextensions(sc: &mut Scanner) {
    for root in GPEXT_ROOTS {
        for sub in reg_subkeys(hkey_local(), root) {
            let full = format!("{root}\\{sub}");
            let dll = reg_get_value(hkey_local(), &full, "DllName").unwrap_or_default();
            if dll.trim().is_empty() {
                continue;
            }
            let title = reg_get_value(hkey_local(), &full, "")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(sub.clone());
            let path = resolve_pe_path(&dll);
            sc.push_entry(
                Entry::new(
                    "登录",
                    &title,
                    format!("[GPExtension] {path}"),
                    "info",
                    Some(path),
                )
                .op_key(format!("reg\u{1f}HKLM\u{1f}{full}\u{1f}DllName"))
                .desc(sub),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 登录：Active Setup（StubPath——装机后每用户触发一次，常被滥用）
