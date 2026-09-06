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

/// Linux：systemd（list-units 全量 + list-unit-files 取启动类型）。
fn list_systemd() -> (Vec<SysServiceEntry>, Option<String>) {
    let out = crate::child::quiet("systemctl")
        .args([
            "list-units",
            "--type=service",
            "--all",
            "--no-legend",
            "--no-pager",
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

    // 启动类型：unit-file state（enabled→auto / disabled→disabled / static→manual）
    let mut start_types = std::collections::HashMap::new();
    if let Ok(o) = crate::child::quiet("systemctl")
        .args([
            "list-unit-files",
            "--type=service",
            "--no-legend",
            "--no-pager",
        ])
        .output()
        && o.status.success()
    {
        for line in crate::encoding::decode_console(&o.stdout).lines() {
            let mut cols = line.split_whitespace();
            if let (Some(unit), Some(state)) = (cols.next(), cols.next()) {
                start_types.insert(unit.to_string(), map_unit_file_state(state));
            }
        }
    }

    let services = text
        .lines()
        .filter_map(|line| {
            // UNIT LOAD ACTIVE SUB DESCRIPTION
            let mut cols = line.split_whitespace();
            let name = cols.next()?.to_string();
            let _load = cols.next()?;
            let active = cols.next().unwrap_or("");
            cols.next(); // sub
            let description = cols.collect::<Vec<_>>().join(" ");
            let status = match active {
                "active" => "running".to_string(),
                "failed" => "failed".to_string(),
                _ => "stopped".to_string(),
            };
            Some(SysServiceEntry {
                start_type: start_types.get(&name).cloned().unwrap_or_default(),
                name,
                display_name: String::new(),
                status,
                pid: 0,
                description,
            })
        })
        .collect();
    (services, None)
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
