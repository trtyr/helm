//! WMI 永久事件订阅（Autoruns "WMI" 标签页）：
//! root\subscription 下 __EventFilter + __EventConsumer + FilterToConsumerBinding。
//! 正常系统几乎为空——出现即为高价值 IR 信号。

use super::util::{Scanner, child_cmd, decode_console, extract_exe};

pub fn scan(sc: &mut Scanner) {
    let script = concat!(
        "Get-CimInstance -Namespace root\\subscription -ClassName __FilterToConsumerBinding | ",
        "ForEach-Object { \"{0}`t{1}`t{2}`t{3}\" -f $_.Filter.Name, $_.Filter.Query, ",
        "$_.Consumer.CimClass.CimClassName, $_.Consumer.CommandLineTemplate }"
    );
    let output = child_cmd("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .output();
    let Ok(o) = output else { return };
    if !o.status.success() {
        return;
    }
    let text = decode_console(&o.stdout);
    for line in text.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 3 {
            continue;
        }
        let filter = parts[0].trim();
        let query = parts[1].trim();
        let consumer_cls = parts[2].trim();
        let cmd = parts.get(3).copied().unwrap_or("").trim();
        if filter.is_empty() && cmd.is_empty() {
            continue;
        }
        // Windows 内置订阅（每台系统都有，非 IR 信号）
        if filter.eq_ignore_ascii_case("SCM Event Log Filter")
            && consumer_cls.eq_ignore_ascii_case("NTEventLogEventConsumer")
        {
            continue;
        }
        let exe = extract_exe(cmd);
        // 命令行/脚本消费器 = 直接代码执行， critical
        let sev = if consumer_cls.contains("EventConsumer") && !cmd.is_empty() {
            "critical"
        } else {
            "warn"
        };
        sc.push(
            "WMI 订阅",
            filter,
            format!("Consumer: {consumer_cls} | Query: {query} | Cmd: {cmd}"),
            sev,
            exe,
            Some(consumer_cls.to_string()),
        );
    }
}
