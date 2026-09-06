//! 进程管理 + 网络信息采集（进程列表对标 Process Hacker：CPU/内存/属主/父进程/命令行）。

use std::collections::HashMap;
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicBool, Ordering},
};

use helm_proto::pb::{
    AgentMessage, NetConnection, NetInfoResult, NetInterface, ProcessInfo, ProcessKillResult,
    ProcessListResult, agent_message,
};

/// 常驻共享的 System 快照：CPU 使用率是「两次刷新的时间差」，
/// 必须跨请求保留基线，否则永远为 0（Process Hacker 同样是常驻快照周期刷新）。
fn shared_system() -> &'static Mutex<sysinfo::System> {
    static SYS: OnceLock<Mutex<sysinfo::System>> = OnceLock::new();
    SYS.get_or_init(|| Mutex::new(sysinfo::System::new_all()))
}

/// 列出进程，返回 ProcessListResult 消息。
pub fn list_processes(request_id: &str) -> AgentMessage {
    let mut sys = shared_system().lock().unwrap();

    // 首次请求：补一次最小测量窗口（sysinfo 要求两次刷新间隔 ≥ 200ms）
    static FIRST: AtomicBool = AtomicBool::new(true);
    let first = FIRST.swap(false, Ordering::Relaxed);
    if first {
        std::thread::sleep(std::time::Duration::from_millis(250));
    }

    // 全量刷新并取完整信息（exe/cmd/user + CPU/内存差值）
    sys.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::everything(),
    );

    let users = sysinfo::Users::new_with_refreshed_list();

    let mut processes: Vec<ProcessInfo> = sys
        .processes()
        .iter()
        .map(|(pid, p)| {
            // 属主：经 uid 在用户表中反查名字
            let user = p
                .user_id()
                .and_then(|uid| users.iter().find(|u| u.id() == uid))
                .map(|u| u.name().to_string())
                .unwrap_or_default();
            let start_time_unix = p.start_time();
            // sysinfo 的 start_time 为 0 表示未知
            let pid_u32 = pid.as_u32();
            let exe_path = p
                .exe()
                .map(|e| e.to_string_lossy().to_string())
                .unwrap_or_default();
            // sysinfo 常对系统进程拿不到路径：原生 API 兜底再查一次
            let exe_path = if exe_path.is_empty() {
                native_image_path(pid_u32).unwrap_or_default()
            } else {
                exe_path
            };
            ProcessInfo {
                pid: pid_u32 as i32,
                name: p.name().to_string_lossy().to_string(),
                cpu_percent: p.cpu_usage(),
                mem_bytes: p.memory(),
                virt_mem_bytes: p.virtual_memory(),
                parent_pid: p.parent().map(|pp| pp.as_u32() as i32).unwrap_or(0),
                status: p.status().to_string(),
                user,
                start_time_unix,
                exe_path,
                cmd: p
                    .cmd()
                    .iter()
                    .map(|c| c.to_string_lossy().to_string())
                    .collect::<Vec<_>>()
                    .join(" "),
            }
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

/// 进程绝对路径查询（Windows 原生兜底；其他平台无兜底）。
fn native_image_path(pid: u32) -> Option<String> {
    #[cfg(windows)]
    return crate::win_native::process_image_path(pid);
    #[cfg(not(windows))]
    {
        let _ = pid;
        None
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

/// 采集网络信息（接口 + TCP/UDP 连接表），返回 NetInfoResult 消息。
pub fn net_info(request_id: &str) -> AgentMessage {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_default();

    let interfaces = collect_interfaces();
    let connections = collect_connections();
    AgentMessage {
        kind: Some(agent_message::Kind::NetInfoResult(NetInfoResult {
            request_id: request_id.to_string(),
            hostname,
            interfaces,
            connections,
        })),
    }
}

/// 网络接口：Windows 走 GetAdaptersAddresses 原生枚举（全量适配器 + MAC/网关/状态），
/// 其他平台走 sysinfo（仅名称与地址）。
fn collect_interfaces() -> Vec<NetInterface> {
    #[cfg(windows)]
    {
        crate::win_native::list_adapters()
            .into_iter()
            .map(|(name, addrs, mac, status, gateway, kind)| NetInterface {
                name,
                addrs,
                mac,
                status,
                gateway,
                kind,
            })
            .collect()
    }
    #[cfg(not(windows))]
    {
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
                mac: String::new(),
                status: String::new(),
                gateway: String::new(),
                kind: String::new(),
            })
            .collect();
        interfaces.sort_by(|a, b| a.name.cmp(&b.name));
        interfaces
    }
}

/// 采集 TCP/UDP 连接表：Windows 走原生路由表 API，Unix 走 ss（回退 netstat）。
fn collect_connections() -> Vec<NetConnection> {
    #[cfg(windows)]
    {
        let mut conns = crate::win_native::list_connections();
        attach_process_names(&mut conns);
        conns
    }
    #[cfg(not(windows))]
    {
        let out = crate::child::quiet("ss").args(["-tunap"]).output();
        let mut conns = match out {
            Ok(o) if o.status.success() => parse_ss(&String::from_utf8_lossy(&o.stdout)),
            _ => {
                let out = crate::child::quiet("netstat").args(["-anv"]).output();
                match out {
                    Ok(o) if o.status.success() => {
                        parse_netstat_unix(&String::from_utf8_lossy(&o.stdout))
                    }
                    _ => Vec::new(),
                }
            }
        };
        attach_process_names(&mut conns);
        conns
    }
}

/// 解析 Windows `netstat -ano`（Windows 已走原生 API，此解析保留给 Unix 平台构建的引用与测试）。
#[cfg_attr(windows, allow(dead_code))]
pub fn parse_netstat_windows(text: &str) -> Vec<NetConnection> {
    let mut conns = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 4 || tokens[0].eq_ignore_ascii_case("proto") {
            continue;
        }
        let proto = tokens[0].to_ascii_lowercase();
        if proto != "tcp" && proto != "udp" {
            continue;
        }
        let (state, pid) = if proto == "tcp" && tokens.len() >= 5 {
            (tokens[3].to_string(), tokens[4])
        } else {
            (String::new(), tokens[tokens.len() - 1])
        };
        let pid: i32 = pid.parse().unwrap_or(0);
        conns.push(NetConnection {
            protocol: proto,
            local: tokens[1].to_string(),
            remote: tokens[2].to_string(),
            state,
            pid,
            process_name: String::new(),
        });
    }
    conns
}

/// 解析 Linux `ss -tunap` 行：`tcp ESTAB 0 0 local remote users:(("name",pid=1,fd=2))`。
#[cfg_attr(windows, allow(dead_code))]
pub fn parse_ss(text: &str) -> Vec<NetConnection> {
    let mut conns = Vec::new();
    for line in text.lines().skip(1) {
        // Netid State Recv-Q Send-Q Local-Address:Port Peer-Address:Port [Process]
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 6 {
            continue;
        }
        let proto = tokens[0].to_ascii_lowercase();
        if proto != "tcp" && proto != "udp" {
            continue;
        }
        let state = if proto == "tcp" {
            tokens[1].to_ascii_uppercase()
        } else {
            String::new()
        };
        let (local, remote) = (tokens[4].to_string(), tokens[5].to_string());
        let (pid, pname) = parse_ss_process(line);
        conns.push(NetConnection {
            protocol: proto,
            local,
            remote,
            state,
            pid,
            process_name: pname,
        });
    }
    conns
}

/// 从 ss 行尾的 users:(("name",pid=123,fd=4)) 提取 pid 与进程名。
#[cfg_attr(windows, allow(dead_code))]
fn parse_ss_process(line: &str) -> (i32, String) {
    let Some(idx) = line.find("users:((") else {
        return (0, String::new());
    };
    let rest = &line[idx..];
    let name = rest.split('"').nth(1).unwrap_or_default().to_string();
    let pid = rest
        .split("pid=")
        .nth(1)
        .and_then(|s| s.split([',', ')']).next())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    (pid, name)
}

/// 解析 Unix `netstat -anv`（无 pid 也能列连接；macOS 部分行带 pid）。
#[cfg_attr(windows, allow(dead_code))]
pub fn parse_netstat_unix(text: &str) -> Vec<NetConnection> {
    let mut conns = Vec::new();
    for line in text.lines() {
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 4 {
            continue;
        }
        let proto = match tokens[0].to_ascii_lowercase().as_str() {
            "tcp" | "tcp4" | "tcp6" => "tcp".to_string(),
            "udp" | "udp4" | "udp6" => "udp".to_string(),
            _ => continue,
        };
        // 找两个地址列：第一个是 local，第二个是 remote（扣除头部列数差异）
        let addr_tokens: Vec<&str> = tokens
            .iter()
            .filter(|t| t.contains('.') && !t.parse::<u64>().is_ok())
            .copied()
            .collect();
        if addr_tokens.len() < 2 {
            continue;
        }
        let state = if proto == "tcp" {
            tokens
                .iter()
                .find(|t| {
                    matches!(
                        t.to_ascii_uppercase().as_str(),
                        "LISTEN" | "ESTABLISHED" | "TIME_WAIT" | "SYN_SENT" | "CLOSE_WAIT"
                    )
                })
                .map(|s| s.to_ascii_uppercase())
                .unwrap_or_default()
        } else {
            String::new()
        };
        conns.push(NetConnection {
            protocol: proto,
            local: addr_tokens[0].to_string(),
            remote: addr_tokens[1].to_string(),
            state,
            pid: 0,
            process_name: String::new(),
        });
    }
    conns
}

/// 为连接表补进程名：pid → name（netstat 有 pid 无 name，ss 已带 name）。
pub fn attach_process_names(conns: &mut [NetConnection]) {
    if conns.is_empty() {
        return;
    }
    let sys = sysinfo::System::new_all();
    let names: HashMap<i32, String> = sys
        .processes()
        .iter()
        .map(|(pid, p)| (pid.as_u32() as i32, p.name().to_string_lossy().to_string()))
        .collect();
    for c in conns.iter_mut() {
        if c.process_name.is_empty()
            && c.pid > 0
            && let Some(name) = names.get(&c.pid)
        {
            c.process_name = name.clone();
        }
    }
}

/// 跨平台终止进程。
fn kill_by_pid(pid: i32) -> bool {
    #[cfg(windows)]
    {
        crate::child::quiet("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
    #[cfg(not(windows))]
    {
        crate::child::quiet("kill")
            .arg(pid.to_string())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }
}
