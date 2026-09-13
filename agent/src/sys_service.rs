//! 系统服务发现与控制：Windows（原生 SCM）/ systemd / launchctl。
//! 与 service.rs 的「托管服务」（自启子进程）不同，这里面向目标机原生服务管理。

use helm_proto::pb::{
    AgentMessage, SysServiceActionResult, SysServiceEntry, SysServiceListResult, agent_message,
};

/// 枚举系统服务，返回 SysServiceListResult 消息。
pub fn list_services(request_id: &str) -> AgentMessage {
    let (mut services, error) = match std::env::consts::OS {
        "windows" => list_windows(),
        "linux" => list_systemd(),
        "macos" => list_launchctl(),
        _ => (Vec::new(), Some("unsupported platform".to_string())),
    };
    services.sort_by(|a, b| a.name.cmp(&b.name));
    AgentMessage {
        kind: Some(agent_message::Kind::SysServiceListResult(
            SysServiceListResult {
                request_id: request_id.to_string(),
                services,
                error,
            },
        )),
    }
}

/// 对系统服务执行 start/stop/restart，返回 SysServiceActionResult 消息。
pub fn service_action(request_id: &str, name: &str, action: &str) -> AgentMessage {
    let result = run_action(name, action);
    AgentMessage {
        kind: Some(agent_message::Kind::SysServiceActionResult(
            SysServiceActionResult {
                request_id: request_id.to_string(),
                ok: result.is_ok(),
                error: result.err(),
            },
        )),
    }
}

fn run_action(name: &str, action: &str) -> Result<(), String> {
    let action = action.to_ascii_lowercase();
    if !matches!(action.as_str(), "start" | "stop" | "restart") {
        return Err(format!("unknown action: {action}"));
    }

    // Windows：原生 SCM
    #[cfg(windows)]
    if std::env::consts::OS == "windows" {
        let r = if action == "restart" {
            crate::win_native::service_restart(name)
        } else {
            crate::win_native::service_action(name, &action)
        };
        return r.map_err(|e| format!("{action} {name} failed: {e}"));
    }

    // Unix：systemctl / launchctl 命令（restart = stop → start，stop 失败容忍）
    let steps: Vec<&str> = match action.as_str() {
        "start" => vec!["start"],
        "stop" => vec!["stop"],
        _ => vec!["stop", "start"],
    };
    let mut last_err = None;
    for step in steps {
        let out = if std::env::consts::OS == "macos" {
            crate::child::quiet("launchctl").args([step, name]).output()
        } else {
            crate::child::quiet("systemctl").args([step, name]).output()
        };
        match out {
            Ok(o) if o.status.success() => last_err = None,
            Ok(o) => {
                last_err = Some(format!(
                    "{step} {name} failed: {}",
                    crate::encoding::decode_console(&o.stderr).trim()
                ));
                if step == "stop" && action == "restart" {
                    continue; // 已停止的服务 restart 允许 stop 失败
                }
                break;
            }
            Err(e) => {
                last_err = Some(format!("{step} execution failed: {e}"));
                break;
            }
        }
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Windows：原生 SCM 枚举（SCM 直出 UTF-16，无编码问题、无外部进程）。
#[cfg(windows)]
fn list_windows() -> (Vec<SysServiceEntry>, Option<String>) {
    match crate::win_native::list_services() {
        Ok(list) => (list, None),
        Err(e) => (Vec::new(), Some(format!("SCM 枚举失败: {e}"))),
    }
}

/// 非 Windows 平台编译占位。
#[cfg(not(windows))]
fn list_windows() -> (Vec<SysServiceEntry>, Option<String>) {
    (Vec::new(), Some("unsupported platform".to_string()))
}

/// Linux：systemd。单次 `systemctl show` 拿全量属性（Id/ActiveState/MainPID/
/// UnitFileState/FragmentPath/Description/ActiveEnterTimestampMonotonic），
/// unit 之间以空行分块。since 由 monotonic 微秒 + /proc/stat 的 btime 换算
/// 成 unix 秒（时区无关）。
fn list_systemd() -> (Vec<SysServiceEntry>, Option<String>) {
    let out = crate::child::quiet("systemctl")
        .args([
            "show",
            "--type=service",
            "--all",
            "--no-pager",
            "--property=Id,ActiveState,MainPID,UnitFileState,FragmentPath,Description,ActiveEnterTimestampMonotonic",
        ])
        .output();
    let text = match out {
        Ok(o) if o.status.success() => crate::encoding::decode_console(&o.stdout),
        Ok(o) => {
            return (
                Vec::new(),
                Some(format!(
                    "systemctl failed: {}",
                    crate::encoding::decode_console(&o.stderr).trim()
                )),
            );
        }
        Err(e) => return (Vec::new(), Some(format!("systemctl spawn failed: {e}"))),
    };

    let boot_unix = proc_stat_btime();
    let services = parse_systemd_show(&text, boot_unix);
    (services, None)
}

/// 解析 `systemctl show` 输出：Key=Value 行，unit 块以空行分隔（纯函数，便于测试）。
fn parse_systemd_show(text: &str, boot_unix: u64) -> Vec<SysServiceEntry> {
    let mut services = Vec::new();
    let mut cur: std::collections::HashMap<&str, &str> = std::collections::HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            if cur.is_empty() {
                continue;
            }
            services.push(systemd_entry(&cur, boot_unix));
            cur.clear();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            cur.insert(k, v);
        }
    }
    if !cur.is_empty() {
        services.push(systemd_entry(&cur, boot_unix));
    }
    services
}

/// 由 systemctl show 的属性块构造条目。
fn systemd_entry(props: &std::collections::HashMap<&str, &str>, boot_unix: u64) -> SysServiceEntry {
    let active = props.get("ActiveState").copied().unwrap_or("");
    let status = match active {
        "active" => "running",
        "failed" => "failed",
        _ => "stopped",
    };
    // monotonic 微秒（进入当前状态的时刻，相对开机）→ unix 秒
    let since_unix = props
        .get("ActiveEnterTimestampMonotonic")
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|&us| us > 0 && boot_unix > 0)
        .map(|us| boot_unix + us / 1_000_000)
        .unwrap_or(0);
    let unit_file_state = props.get("UnitFileState").copied().unwrap_or("");
    SysServiceEntry {
        name: props.get("Id").copied().unwrap_or_default().to_string(),
        display_name: String::new(),
        status: status.to_string(),
        // 兼容旧列：enabled/indirect→auto，static→manual，disabled→disabled
        start_type: map_unit_file_state(unit_file_state),
        pid: props
            .get("MainPID")
            .and_then(|v| v.parse::<i32>().ok())
            .unwrap_or(0),
        description: props
            .get("Description")
            .copied()
            .unwrap_or_default()
            .to_string(),
        enabled_state: unit_file_state.to_string(),
        since_unix,
        unit_file: props
            .get("FragmentPath")
            .copied()
            .unwrap_or_default()
            .to_string(),
    }
}

/// /proc/stat 的 btime 行（系统启动的 unix 秒）。读取失败返回 0。
fn proc_stat_btime() -> u64 {
    std::fs::read_to_string("/proc/stat")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("btime "))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
        })
        .unwrap_or(0)
}

fn map_unit_file_state(state: &str) -> String {
    match state {
        "enabled" | "enabled-runtime" | "indirect" => "auto".to_string(),
        "disabled" => "disabled".to_string(),
        "static" => "manual".to_string(),
        other => other.to_string(),
    }
}

/// macOS：launchctl list（PID/Status/Label 三列）。
fn list_launchctl() -> (Vec<SysServiceEntry>, Option<String>) {
    let out = crate::child::quiet("launchctl").arg("list").output();
    match out {
        Ok(o) if o.status.success() => {
            let text = crate::encoding::decode_console(&o.stdout);
            let services = text
                .lines()
                .skip(1) // 表头 PID	Status	Label
                .filter_map(|line| {
                    let mut cols = line.split('\t');
                    let pid = cols.next()?.trim().parse().unwrap_or(0);
                    let _status_code = cols.next()?;
                    let label = cols.next()?.trim().to_string();
                    if label.is_empty() {
                        return None;
                    }
                    Some(SysServiceEntry {
                        name: label.clone(),
                        display_name: label,
                        status: if pid > 0 { "running" } else { "stopped" }.to_string(),
                        start_type: String::new(),
                        pid,
                        description: String::new(),
                        enabled_state: String::new(),
                        since_unix: 0,
                        unit_file: String::new(),
                    })
                })
                .collect();
            (services, None)
        }
        Ok(o) => (
            Vec::new(),
            Some(format!(
                "launchctl failed: {}",
                crate::encoding::decode_console(&o.stderr).trim()
            )),
        ),
        Err(e) => (Vec::new(), Some(format!("launchctl spawn failed: {e}"))),
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn parse_systemd_show_blocks() {
        let text = "\
Id=accounts-daemon.service
ActiveState=active
MainPID=749
UnitFileState=enabled
FragmentPath=/usr/lib/systemd/system/accounts-daemon.service
Description=Account Service
ActiveEnterTimestampMonotonic=123807040

Id=alsa-restore.service
ActiveState=inactive
MainPID=0
UnitFileState=
FragmentPath=
Description=Save/Restore Sound Card State
ActiveEnterTimestampMonotonic=0
";
        // btime=1_000_000 → since = 1_000_000 + 123807040us/1e6 = 1_000_123（秒）
        let svcs = super::parse_systemd_show(text, 1_000_000);
        assert_eq!(svcs.len(), 2);
        assert_eq!(svcs[0].name, "accounts-daemon.service");
        assert_eq!(svcs[0].status, "running");
        assert_eq!(svcs[0].start_type, "auto");
        assert_eq!(svcs[0].enabled_state, "enabled");
        assert_eq!(svcs[0].pid, 749);
        assert_eq!(svcs[0].since_unix, 1_000_123);
        assert_eq!(
            svcs[0].unit_file,
            "/usr/lib/systemd/system/accounts-daemon.service"
        );
        assert_eq!(svcs[1].status, "stopped");
        assert_eq!(svcs[1].enabled_state, "");
        assert_eq!(svcs[1].since_unix, 0);
        assert_eq!(svcs[1].pid, 0);
    }

    #[test]
    fn netstat_windows_lines_parse() {
        let text = "\n  Proto  Local Address          Foreign Address        State           PID\n  \
                    TCP    0.0.0.0:135            0.0.0.0:0              LISTENING       984\n  \
                    TCP    127.0.0.1:50051        127.0.0.1:52333        ESTABLISHED     4242\n  \
                    UDP    0.0.0.0:5353           *:*                                    5353\n  \
                    UDP    127.0.0.1:1900         *:*                                    5678\n";
        let conns = crate::process::parse_netstat_windows(text);
        assert_eq!(conns.len(), 4);
        assert_eq!(conns[0].protocol, "tcp");
        assert_eq!(conns[0].state, "LISTENING");
        assert_eq!(conns[0].pid, 984);
        assert_eq!(conns[1].state, "ESTABLISHED");
        assert_eq!(conns[2].protocol, "udp");
        assert_eq!(conns[2].state, "");
        assert_eq!(conns[2].pid, 5353);
        assert_eq!(conns[3].pid, 5678);
    }

    #[test]
    fn ss_lines_parse() {
        let text = "Netid State Recv-Q Send-Q Local-Address:Port Peer-Address:Port Process\n\
                    tcp   ESTAB 0      0      10.0.0.2:22        10.0.0.5:51023     users:((\"sshd\",pid=1200,fd=3))\n\
                    udp   UNCONN 0     0      0.0.0.0:68         0.0.0.0:*\n";
        let conns = crate::process::parse_ss(text);
        assert_eq!(conns.len(), 2);
        assert_eq!(conns[0].protocol, "tcp");
        assert_eq!(conns[0].state, "ESTAB");
        assert_eq!(conns[0].pid, 1200);
        assert_eq!(conns[0].process_name, "sshd");
        assert_eq!(conns[1].protocol, "udp");
        assert_eq!(conns[1].state, "");
        assert_eq!(conns[1].pid, 0);
    }
}
