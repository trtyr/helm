//! 进程管理 + 网络信息采集。

use helm_proto::pb::{
    AgentMessage, NetInfoResult, NetInterface, ProcessInfo, ProcessKillResult, ProcessListResult,
    agent_message,
};

/// 列出进程，返回 ProcessListResult 消息。
pub fn list_processes(request_id: &str) -> AgentMessage {
    let mut sys = sysinfo::System::new_all();
    sys.refresh_all();
    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| ProcessInfo {
            pid: pid.as_u32() as i32,
            name: p.name().to_string_lossy().to_string(),
            cpu_percent: p.cpu_usage(),
            mem_bytes: p.memory(),
        })
        .collect();
    processes.sort_by_key(|p| p.pid);
    AgentMessage {
        kind: Some(agent_message::Kind::ProcessListResult(ProcessListResult {
            request_id: request_id.to_string(),
            processes,
        })),
    }
}

/// 终止进程，返回 ProcessKillResult 消息。
pub fn kill_process(request_id: &str, pid: i32) -> AgentMessage {
    let ok = kill_by_pid(pid);
    AgentMessage {
        kind: Some(agent_message::Kind::ProcessKillResult(ProcessKillResult {
            request_id: request_id.to_string(),
            pid,
            ok,
            error: if ok {
                None
            } else {
                Some("kill failed (no such process or insufficient permission)".into())
            },
        })),
    }
}

/// 采集网络信息，返回 NetInfoResult 消息。
pub fn net_info(request_id: &str) -> AgentMessage {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();
    let networks = sysinfo::Networks::new_with_refreshed_list();
    let mut interfaces: Vec<NetInterface> = networks
        .iter()
        .map(|(name, data)| NetInterface {
            name: name.clone(),
            addrs: data
                .ip_networks()
                .iter()
                .map(|ip| ip.addr.to_string())
                .collect(),
        })
        .collect();
    interfaces.sort_by(|a, b| a.name.cmp(&b.name));
    AgentMessage {
        kind: Some(agent_message::Kind::NetInfoResult(NetInfoResult {
            request_id: request_id.to_string(),
            hostname,
            interfaces,
        })),
    }
}

/// 跨平台终止进程。
fn kill_by_pid(pid: i32) -> bool {
    #[cfg(windows)]
    {
        std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("kill")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_proto::pb::agent_message;

    #[test]
    fn kill_nonexistent_pid_returns_false() {
        match kill_process("r", 999999).kind {
            Some(agent_message::Kind::ProcessKillResult(r)) => assert!(!r.ok),
            _ => panic!("expected ProcessKillResult"),
        }
    }

    #[test]
    fn list_processes_returns_entries() {
        match list_processes("r").kind {
            Some(agent_message::Kind::ProcessListResult(r)) => assert!(!r.processes.is_empty()),
            _ => panic!("expected ProcessListResult"),
        }
    }

    #[test]
    fn net_info_returns_hostname() {
        match net_info("r").kind {
            Some(agent_message::Kind::NetInfoResult(r)) => assert!(!r.hostname.is_empty()),
            _ => panic!("expected NetInfoResult"),
        }
    }
}
