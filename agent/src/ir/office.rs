//! Office 持久化（Autoruns "Office" 标签页）：
//! COM 加载项（Word/Excel/PowerPoint/Outlook/Access）、Word 启动文件夹（WLL）、Excel XLL（OPEN 值）。

use super::util::{
    Entry, Scanner, clsid_server, extract_exe, hkey_local, reg_get_value, reg_subkeys, reg_values,
};
use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;

const OFFICE_APPS: &[&str] = &["Word", "Excel", "PowerPoint", "Outlook", "Access"];

pub fn scan(sc: &mut Scanner) {
    com_addins(sc);
    word_startup_folder(sc);
    excel_xll(sc);
}

/// COM 加载项：SOFTWARE\Microsoft\Office\<版本>\<应用>\Addins\<ProgID>。
fn com_addins(sc: &mut Scanner) {
    for hive_label in ["HKLM", "HKCU"] {
        let root_hive = if hive_label == "HKLM" {
            hkey_local()
        } else {
            HKEY_CURRENT_USER
        };
        let office_root = r"SOFTWARE\Microsoft\Office";
        for ver in reg_subkeys(root_hive, office_root) {
            // 版本子键形如 16.0 / 15.0（跳过非版本键如 Registration）
            if !ver
                .chars()
                .next()
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false)
            {
                continue;
            }
            for app in OFFICE_APPS {
                let addins_root = format!("{office_root}\\{ver}\\{app}\\Addins");
                for id in reg_subkeys(root_hive, &addins_root) {
                    let full = format!("{addins_root}\\{id}");
                    let get = |k: &str| {
                        reg_get_value(root_hive, &full, k).filter(|s| !s.trim().is_empty())
                    };
                    let friendly = get("FriendlyName");
                    let load_behavior = get("LoadBehavior").unwrap_or_default();
                    let manifest = get("Manifest");
                    // DLL 解析：ProgID → CLSID → InprocServer32；VSTO 走 Manifest
                    let path = clsid_server(&id).or_else(|| {
                        manifest.as_ref().and_then(|m| {
                            let m = m.trim().trim_matches('"');
                            (!m.is_empty()).then(|| m.to_string())
                        })
                    });
                    let load_note = match load_behavior.trim() {
                        "0" => "未加载",
                        "2" => "启动加载",
                        "3" => "启动加载",
                        "8" | "9" | "16" | "22" => "按需加载",
                        "" => "?",
                        _ => "其他",
                    };
                    sc.push_entry(
                        Entry::new(
                            "Office",
                            friendly.as_deref().unwrap_or(&id),
                            format!(
                                "[{hive_label}\\{ver}\\{app} Addin] {id} | LoadBehavior: {} ({load_note})",
                                load_behavior.trim()
                            ),
                            "info",
                            path,
                        )
                        .desc(id),
                    );
                }
            }
        }
    }
}

/// Word 启动文件夹（*.wll 自动加载）。
fn word_startup_folder(sc: &mut Scanner) {
    let mut dirs: Vec<String> = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        dirs.push(format!(r"{appdata}\Microsoft\Word\Startup"));
    }
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        for e in rd.flatten() {
            let path = e.path().to_string_lossy().into_owned();
            sc.push_entry(Entry::new(
                "Office",
                &e.file_name().to_string_lossy(),
                format!("[Word 启动文件夹] {path}"),
                "info",
                Some(path),
            ));
        }
    }
}

/// Excel XLL：HKCU...\Excel\Options 的 OPEN / OPEN_MULTIPLE / OPEN_ONCE 值。
fn excel_xll(sc: &mut Scanner) {
    let office_root = r"SOFTWARE\Microsoft\Office";
    for ver in reg_subkeys(HKEY_CURRENT_USER, office_root) {
        let options = format!("{office_root}\\{ver}\\Excel\\Options");
        for (name, val) in reg_values(HKEY_CURRENT_USER, &options) {
            if !name.starts_with("OPEN") || val.trim().is_empty() {
                continue;
            }
            let exe = extract_exe(&val);
            sc.push_entry(Entry::new(
                "Office",
                &val,
                format!("[Excel XLL\\{ver}] {name}: {val}"),
                "info",
                exe,
            ));
        }
    }
}
