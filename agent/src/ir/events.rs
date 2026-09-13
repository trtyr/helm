//! 事件日志审计：登录（4624/4625）/建户（4720）/日志清除（1102）/装服务（7045）/PowerShell 脚本块（4104）。

use super::util::{Scanner, child_cmd, decode_console};

pub fn scan(sc: &mut Scanner) {
    // Security 4624/4625/4720/1102（需管理员）
    let sec = wevt_events("Security", "4624 or 4625 or 4720 or 1102", 40);
    if sec.is_empty() {
        sc.push_raw(
            "事件日志",
            "Security 日志",
            "不可读（需管理员）或无事件",
            "warn",
        );
    }
    for (id, time, msg) in &sec {
        let sev = match id.as_str() {
            "1102" => "critical",
            "4720" => "warn",
            _ => "info",
        };
        sc.push_raw_ts(
            "事件日志",
            &format!("事件 {id}"),
            format!("[{time}] {msg}"),
            sev,
            iso_to_unix(time),
        );
    }
    // System 7045 装服务
    let sys = wevt_events("System", "7045", 20);
    for (id, time, msg) in &sys {
        sc.push_raw_ts(
            "事件日志",
            &format!("事件 {id}"),
            format!("[{time}] {msg}"),
            "warn",
            iso_to_unix(time),
        );
    }
    // PowerShell 4104 脚本块
    let ps = wevt_events("Windows PowerShell", "4104", 20);
    for (id, time, msg) in &ps {
        sc.push_raw_ts(
            "事件日志",
            &format!("事件 {id}"),
            format!("[{time}] {msg}"),
            "info",
            iso_to_unix(time),
        );
    }
}

/// wevtutil 输出的 ISO 时间（"2026-09-05T12:43:49.3500000Z"）→ unix 秒。
/// 手写天序算法（Hinnant），避免为单一解析引入 chrono。
pub fn iso_to_unix(s: &str) -> i64 {
    let num = |a: usize, b: usize| s.get(a..b).and_then(|x| x.parse::<i64>().ok());
    let (Some(y), Some(mo), Some(d)) = (num(0, 4), num(5, 7), num(8, 10)) else {
        return 0;
    };
    let (Some(h), Some(mi), Some(sec)) = (num(11, 13), num(14, 16), num(17, 19)) else {
        return 0;
    };
    let y2 = if mo <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = if mo > 2 { mo - 3 } else { mo + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146097 + doe - 719468;
    days * 86400 + h * 3600 + mi * 60 + sec
}

fn wevt_events(log: &str, ids: &str, count: u32) -> Vec<(String, String, String)> {
    let xpath = format!("*[System[(EventID={ids})]]");
    let count_arg = format!("/c:{count}");
    let query_arg = format!("/q:{xpath}");
    let output = child_cmd("wevtutil")
        .args(["qe", log, count_arg.as_str(), "/rd:true", "/f:text"])
        .arg(query_arg)
        .output();
    let Ok(o) = output else { return Vec::new() };
    if !o.status.success() {
        return Vec::new();
    }
    let text = decode_console(&o.stdout);
    let mut events = Vec::new();
    for block in text.split("\r\n\r\n") {
        let mut id = String::new();
        let mut date = String::new();
        let mut desc = String::new();
        for line in block.lines() {
            let t = line.trim();
            if let Some(v) = t.strip_prefix("Event ID:") {
                id = v.trim().to_string();
            } else if let Some(v) = t.strip_prefix("Date:") {
                date = v.trim().to_string();
            } else if !t.is_empty()
                && !t.starts_with("Log Name:")
                && !t.starts_with("Source:")
                && !t.starts_with("Computer")
                && !t.starts_with("Event[")
                && desc.len() < 200
            {
                desc.push_str(t);
                desc.push(' ');
            }
        }
        if !id.is_empty() {
            events.push((id, date, desc));
        }
    }
    events
}
