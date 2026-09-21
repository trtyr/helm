//! 命令行 → 可执行文件路径：加载器（rundll32 等）载荷提取、内核/服务风格路径归一化。
//!
//! 自 `ir/util.rs` 拆出（G7）。原分节标题：「命令行 → 可执行文件路径」。
//! 环境变量展开已于 T4 迁往 [`crate::ir::text::expand_env`]（纯字符串逻辑，不该被 Windows 门控）。

use crate::ir::text::expand_env;

fn file_exists(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// 从命令行提取可执行/DLL 路径：
/// `"C:\a b\x.exe" -arg`、`C:\a\x.exe -arg`、`rundll32.exe foo.dll,Entry`。
/// 返回前经 resolve_pe_path 归一化相对/内核风格路径。
pub fn extract_exe(cmdline: &str) -> Option<String> {
    let s = cmdline.trim();
    if s.is_empty() {
        return None;
    }
    let (first, rest) = if let Some(r) = s.strip_prefix('"') {
        match r.find('"') {
            Some(i) => (r[..i].to_string(), r[i + 1..].trim().to_string()),
            None => (s.replace('"', ""), String::new()),
        }
    } else {
        match s.find(' ') {
            Some(i) => (s[..i].to_string(), s[i..].trim().to_string()),
            None => (s.to_string(), String::new()),
        }
    };

    // 加载器（rundll32/regsvr32/脚本宿主等）：实际载荷是第一个参数
    let base = first
        .to_ascii_lowercase()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_string();
    const LOADERS: &[&str] = &[
        "rundll32.exe",
        "regsvr32.exe",
        "mshta.exe",
        "wscript.exe",
        "cscript.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
    ];
    if LOADERS.contains(&base.as_str()) && !rest.is_empty() {
        let rest_trim = rest.trim_start_matches('@');
        let tok = if let Some(r2) = rest_trim.strip_prefix('"') {
            r2.split('"').next().unwrap_or("").to_string()
        } else {
            rest_trim.split([' ', ',']).next().unwrap_or("").to_string()
        };
        let tok = resolve_pe_path(&expand_env(&tok));
        if file_exists(&tok) {
            return Some(tok);
        }
        let tok2 = resolve_pe_path(tok.split(',').next().unwrap_or(""));
        if file_exists(&tok2) {
            return Some(tok2);
        }
    }

    let p = resolve_pe_path(&expand_env(&first));
    if file_exists(&p) {
        return Some(p);
    }
    // 路径含空格但未加引号：逐 token 拼接尝试
    let mut acc = first.clone();
    for tok in rest.split(' ') {
        if tok.is_empty() {
            continue;
        }
        acc.push(' ');
        acc.push_str(tok);
        let cand = resolve_pe_path(&expand_env(acc.trim()));
        if file_exists(&cand) {
            return Some(cand);
        }
    }
    if p.contains('\\') || p.contains('/') {
        return Some(p); // 保底：返回原样供展示
    }
    None
}

/// 归一化注册表中的可执行路径：展开 `\SystemRoot`、`\??\`、裸 DLL 名等
/// 内核/服务风格路径为绝对路径（找不到则以最可能的候选返回）。
pub fn resolve_pe_path(raw: &str) -> String {
    // 体检新提项（T5）：先过滤 `..` 上跳段——注册表路径可能是攻击者可控字符串
    let sanitized = crate::ir::text::strip_parent_dir_segments(raw.trim());
    let raw = sanitized.trim_matches('"');
    if raw.is_empty() {
        return String::new();
    }
    let p = raw
        .replace("\\SystemRoot\\", "C:\\Windows\\")
        .replace("\\SystemRoot", "C:\\Windows");
    let p = p.strip_prefix("\\??\\").map(|s| s.to_string()).unwrap_or(p);
    let p = expand_env(&p);
    let lower = p.to_ascii_lowercase();
    if lower.starts_with("system32\\") {
        return format!(r"C:\Windows\{p}");
    }
    if p.starts_with('\\') && !p.starts_with("\\\\") {
        return format!("C:{p}");
    }
    if !p.contains(':') && !p.starts_with('\\') {
        // 裸文件名或相对路径：按 System32 → drivers → Windows 顺序探测
        for prefix in [
            r"C:\Windows\System32\",
            r"C:\Windows\System32\drivers\",
            r"C:\Windows\",
        ] {
            let cand = format!("{prefix}{p}");
            if file_exists(&cand) {
                return cand;
            }
        }
    }
    p
}
