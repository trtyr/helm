//! 浏览器持久化（Autoruns "Internet Explorer" 标签页扩展）：
//! BHO、IE 工具栏、Chrome/Edge/Firefox 扩展。

use super::util::{Scanner, clsid_server, hkey_local, reg_get_value, reg_subkeys};
use windows_sys::Win32::System::Registry::{HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

pub fn scan(sc: &mut Scanner) {
    bho(sc);
    chromium_extensions(
        sc,
        "Chrome",
        &[
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Google\Chrome\Extensions"),
            (HKEY_CURRENT_USER, r"SOFTWARE\Google\Chrome\Extensions"),
        ],
    );
    chromium_extensions(
        sc,
        "Edge",
        &[
            (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Edge\Extensions"),
            (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Edge\Extensions"),
        ],
    );
    firefox_extensions(sc);
}

/// BHO（Browser Helper Objects）：资源管理器/IE 进程内加载的 DLL。
fn bho(sc: &mut Scanner) {
    const ROOTS: &[&str] = &[
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Browser Helper Objects",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Explorer\Browser Helper Objects",
    ];
    for root in ROOTS {
        for clsid in reg_subkeys(hkey_local(), root) {
            let name = clsid_name(&clsid);
            let path = clsid_server(&clsid);
            sc.push(
                "浏览器",
                &name.unwrap_or_else(|| clsid.clone()),
                format!("[BHO] {clsid}"),
                "info",
                path,
                None,
            );
        }
    }
}

/// Chromium 系（Chrome/Edge）扩展：HKLM/HKCU 扩展注册表。
fn chromium_extensions(sc: &mut Scanner, label: &str, roots: &[(HKEY, &str)]) {
    for (hive, root) in roots {
        for id in reg_subkeys(*hive, root) {
            let full = format!("{root}\\{id}");
            let path_val = reg_get_value(*hive, &full, "path").unwrap_or_default();
            let name = reg_get_value(*hive, &full, "name")
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| id.clone());
            let path =
                if path_val.trim().is_empty() || std::path::Path::new(path_val.trim()).is_file() {
                    if path_val.trim().is_empty() {
                        None
                    } else {
                        Some(path_val.trim().to_string())
                    }
                } else {
                    // 相对路径（相对扩展安装目录）——展示原样
                    Some(path_val.trim().to_string())
                };
            let update_url = reg_get_value(*hive, &full, "update_url").unwrap_or_default();
            sc.push(
                "浏览器",
                &name,
                format!("[{label} 扩展] {id} {update_url}"),
                "info",
                path,
                None,
            );
        }
    }
}

/// Firefox 扩展：解析 profiles.ini + 各 profile 的 extensions.json。
fn firefox_extensions(sc: &mut Scanner) {
    let Some(appdata) = std::env::var("APPDATA").ok() else {
        return;
    };
    let ini = std::path::PathBuf::from(format!(r"{appdata}\Mozilla\Firefox\profiles.ini"));
    let Ok(ini_text) = std::fs::read_to_string(ini) else {
        return;
    };
    let mut profiles: Vec<String> = Vec::new();
    for line in ini_text.lines() {
        let t = line.trim();
        if let Some(p) = t.strip_prefix("Path=") {
            profiles.push(p.trim().to_string());
        }
    }
    for profile in profiles {
        let ext_json = std::path::PathBuf::from(format!(
            r"{appdata}\Mozilla\Firefox\{profile}\extensions.json"
        ));
        let Ok(text) = std::fs::read_to_string(ext_json) else {
            continue;
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let Some(addons) = v["addons"].as_array() else {
            continue;
        };
        for a in addons {
            let name = a["defaultLocale"]["name"]
                .as_str()
                .or_else(|| a["id"].as_str())
                .unwrap_or("?")
                .to_string();
            let path = a["path"].as_str().map(|s| s.to_string());
            let loc = a["location"].as_str().unwrap_or("").to_string();
            sc.push(
                "浏览器",
                &name,
                format!(
                    "[Firefox 扩展] {} ({})",
                    a["id"].as_str().unwrap_or("?"),
                    loc
                ),
                "info",
                path,
                None,
            );
        }
    }
}

fn clsid_name(clsid: &str) -> Option<String> {
    for root in [
        r"SOFTWARE\Classes\CLSID",
        r"SOFTWARE\Classes\Wow6432Node\CLSID",
    ] {
        if let Some(n) = reg_get_value(HKEY_LOCAL_MACHINE, &format!("{root}\\{clsid}"), "")
            && !n.trim().is_empty()
        {
            return Some(n.trim().to_string());
        }
    }
    None
}
