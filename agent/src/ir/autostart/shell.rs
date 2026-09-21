//! 启动文件夹与外壳类持久化（Autoruns "Logon / Explorer" 的 Explorer 侧）：
//! Startup 文件夹（所有用户 + 各用户配置文件，含 AutorunsDisabled 禁用区）、
//! 全量 shellex 处理器、图标覆盖、ShellServiceObjects、ShellExecuteHooks。
//!
//! 自 `autostart.rs` 拆出（G7：该文件生产段 435 行越界）。父模块保留登录类（Run / Active
//! Setup / Winlogon GPExtensions）；本模块只处理「文件系统与外壳扩展」这一面。

use crate::ir::util::{
    AUTORUNS_DISABLED_SUBKEY, Entry, Scanner, clsid_server, hkey_local, reg_get_value, reg_subkeys,
    reg_values,
};

// ---------------------------------------------------------------------------
// 登录：启动文件夹（所有用户 + 各用户配置文件，含 AutorunsDisabled 禁用区）
// ---------------------------------------------------------------------------

const ALL_USERS_STARTUP: &str = r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Startup";

pub(super) fn startup_folders(sc: &mut Scanner) {
    let mut dirs = vec![ALL_USERS_STARTUP.to_string()];
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(format!(
            r"{appdata}\Microsoft\Windows\Start Menu\Programs\Startup"
        ));
    }
    if let Ok(users) = std::fs::read_dir(r"C:\Users") {
        for u in users.flatten() {
            dirs.push(
                u.path()
                    .join(r"AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let path = e.path().to_string_lossy().into_owned();
            let file_name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.path().is_dir();
            // AutorunsDisabled 禁用子文件夹
            if is_dir && file_name == AUTORUNS_DISABLED_SUBKEY {
                let Ok(disabled) = std::fs::read_dir(&path) else {
                    continue;
                };
                for f in disabled.flatten() {
                    let fpath = f.path().to_string_lossy().into_owned();
                    sc.push_entry(
                        Entry::new(
                            "登录",
                            &f.file_name().to_string_lossy(),
                            format!("[启动文件夹·禁用区] {fpath}"),
                            "info",
                            Some(fpath.clone()),
                        )
                        .op_key(format!("file\u{1f}{fpath}"))
                        .disabled(),
                    );
                }
                continue;
            }
            if is_dir {
                continue;
            }
            sc.push_entry(
                Entry::new(
                    "登录",
                    &file_name,
                    format!("[启动文件夹] {path}"),
                    "info",
                    Some(path.clone()),
                )
                .op_key(format!("file\u{1f}{path}")),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 外壳：全量 shellex 处理器 + 图标覆盖 + ShellServiceObjects + ShellExecuteHooks
// ---------------------------------------------------------------------------

/// Explorer 外壳扩展的标准注册点（按对象根 × 处理器类型）。
const SHELLEX_ROOTS: &[&str] = &[
    "*",
    "AllFilesystemObjects",
    "Directory",
    "Directory\\Background",
    "Folder",
    "Drive",
    "exefile",
    "lnkfile",
];
const SHELLEX_TYPES: &[&str] = &[
    "ContextMenuHandlers",
    "CopyHookHandlers",
    "DragDropHandlers",
    "PropertySheetHandlers",
    "ColumnHandlers",
];

pub(super) fn explorer_shell(sc: &mut Scanner) {
    // 全量 shellex 处理器（HKLM 类合并空间，含 Wow6432Node 视图）
    for classes in [r"SOFTWARE\Classes", r"SOFTWARE\Classes\Wow6432Node"] {
        for root in SHELLEX_ROOTS {
            for handler_type in SHELLEX_TYPES {
                let base = format!("{classes}\\{root}\\shellex\\{handler_type}");
                for name in reg_subkeys(hkey_local(), &base) {
                    let full = format!("{base}\\{name}");
                    let clsid = reg_get_value(hkey_local(), &full, "")
                        .filter(|s| !s.trim().is_empty())
                        .unwrap_or(name.clone());
                    let path = clsid_server(clsid.trim());
                    let tag = if classes.contains("Wow6432Node") {
                        "(32)"
                    } else {
                        ""
                    };
                    sc.push_entry(
                        Entry::new(
                            "外壳",
                            &name,
                            format!("[{root}\\{handler_type}{tag}] {clsid}"),
                            "info",
                            path,
                        )
                        .desc(clsid),
                    );
                }
            }
        }
    }

    // ShellIconOverlayIdentifiers：default = CLSID
    for root in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\ShellIconOverlayIdentifiers",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Explorer\ShellIconOverlayIdentifiers",
    ] {
        for name in reg_subkeys(hkey_local(), root) {
            let clsid = reg_get_value(hkey_local(), &format!("{root}\\{name}"), "")
                .unwrap_or_else(|| name.clone());
            let path = clsid_server(clsid.trim());
            sc.push_entry(Entry::new(
                "外壳",
                &name,
                format!("[图标覆盖] CLSID {clsid}"),
                "info",
                path,
            ));
        }
    }

    // ShellServiceObjects：子键名 = CLSID
    for root in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\ShellServiceObjects",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Explorer\ShellServiceObjects",
    ] {
        for clsid in reg_subkeys(hkey_local(), root) {
            let path = clsid_server(&clsid);
            let name = clsid_friendly_name(&clsid).unwrap_or_else(|| clsid.clone());
            sc.push_entry(Entry::new(
                "外壳",
                &name,
                format!("[ShellServiceObject] {clsid}"),
                "info",
                path,
            ));
        }
    }

    // ShellExecuteHooks
    for root in [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\ShellExecuteHooks",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\ShellExecuteHooks",
    ] {
        for (clsid, _) in reg_values(hkey_local(), root) {
            let path = clsid_server(clsid.trim());
            let name = clsid_friendly_name(clsid.trim()).unwrap_or_else(|| clsid.clone());
            sc.push_entry(Entry::new(
                "外壳",
                &name,
                format!("[ShellExecuteHook] {clsid}"),
                "info",
                path,
            ));
        }
    }
}

fn clsid_friendly_name(clsid: &str) -> Option<String> {
    for root in [
        r"SOFTWARE\Classes\CLSID",
        r"SOFTWARE\Classes\Wow6432Node\CLSID",
    ] {
        let name = reg_get_value(hkey_local(), &format!("{root}\\{clsid}"), "");
        if let Some(n) = name
            && !n.trim().is_empty()
        {
            return Some(n.trim().to_string());
        }
    }
    None
}
