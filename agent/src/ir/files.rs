//! 可疑落地文件：临时/公共目录近期可执行文件 + hosts 劫持。

use std::io::Read;

use super::util::Scanner;

pub fn scan(sc: &mut Scanner) {
    const SENSITIVE_DIRS: &[&str] = &[
        r"C:\Windows\Temp",
        r"C:\PerfLogs",
        r"C:\Users\Public",
        r"C:\Windows\Tasks",
    ];
    const EXTS: &[&str] = &[
        "exe", "dll", "bat", "cmd", "ps1", "vbs", "js", "hta", "scr", "pif",
    ];
    let cutoff = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64 - 14 * 86400)
        .unwrap_or(0);

    let mut dirs: Vec<String> = SENSITIVE_DIRS.iter().map(|s| s.to_string()).collect();
    if let Ok(users) = std::fs::read_dir(r"C:\Users") {
        for u in users.flatten() {
            dirs.push(
                u.path()
                    .join(r"AppData\Local\Temp")
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
            let Ok(meta) = e.metadata() else { continue };
            if meta.is_dir() {
                continue;
            }
            let ext = e
                .path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x.to_ascii_lowercase())
                .unwrap_or_default();
            if !EXTS.contains(&ext.as_str()) {
                continue;
            }
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if mtime >= cutoff {
                sc.push_raw(
                    "可疑文件",
                    &e.file_name().to_string_lossy(),
                    format!("{} ({})", e.path().display(), mtime),
                    "warn",
                );
            }
        }
    }

    // hosts
    if let Ok(mut f) = std::fs::File::open(r"C:\Windows\System32\drivers\etc\hosts") {
        let mut content = String::new();
        if f.read_to_string(&mut content).is_ok() {
            let active: Vec<&str> = content
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .collect();
            if !active.is_empty() {
                sc.push_raw("hosts", "hosts 活动条目", active.join("\n"), "info");
            }
        }
    }
}
