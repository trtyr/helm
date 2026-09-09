//! Windows 原生能力（内置能力层，对标 Process Hacker 的实现方式）：
//! - 服务：SCM 原生枚举/启停（替代 PowerShell/CIM，无外部进程、无编码问题）
//! - 网络适配器：GetAdaptersAddressesW（全量接口，含 MAC/网关/状态）
//! - 连接表：GetExtendedTcpTable / GetExtendedUdpTable（含归属 PID，替代 netstat 解析）
//! - 磁盘：GetLogicalDrives / GetDriveTypeW（驱动器枚举，文件管理根视图）
//! - 进程：QueryFullProcessImageNameW（绝对路径兜底查询）
//!
//! 仅 Windows 编译；其他平台调用方自行回退到命令行方案。

#![cfg(windows)]

use std::net::{Ipv4Addr, Ipv6Addr};

use helm_proto::pb::{NetConnection, SysServiceEntry};

use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_BUFFER_OVERFLOW, ERROR_INSUFFICIENT_BUFFER, GetLastError, NO_ERROR,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetExtendedTcpTable, GetExtendedUdpTable,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
use windows_sys::Win32::System::Services::{
    CloseServiceHandle, ControlService, ENUM_SERVICE_STATUS_PROCESSW, EnumServicesStatusExW,
    OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW, QueryServiceConfigW, QueryServiceStatus,
    SC_ENUM_PROCESS_INFO, SC_HANDLE, SC_MANAGER_CONNECT, SC_MANAGER_ENUMERATE_SERVICE,
    SERVICE_AUTO_START, SERVICE_BOOT_START, SERVICE_CONTROL_STOP, SERVICE_DEMAND_START,
    SERVICE_DISABLED, SERVICE_QUERY_CONFIG, SERVICE_RUNNING, SERVICE_START, SERVICE_STATE_ALL,
    SERVICE_STATUS, SERVICE_STOP, SERVICE_STOPPED, SERVICE_SYSTEM_START, SERVICE_WIN32,
    StartServiceW,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

// ---------------------------------------------------------------------------
// 服务（SCM）
// ---------------------------------------------------------------------------

/// 枚举全部系统服务（状态 + 配置），字段与 SysServiceEntry 对齐。
pub fn list_services() -> anyhow::Result<Vec<SysServiceEntry>> {
    let scm = open_scm(SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE)?;

    // 枚举：状态 + PID（EnumServicesStatusExW 双次调用模式）
    let mut needed: u32 = 0;
    let mut returned: u32 = 0;
    let mut size: u32 = 64 * 1024;
    let (buffer, count) = loop {
        let mut buffer = vec![0u8; size as usize];
        let rc = unsafe {
            EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                buffer.as_mut_ptr(),
                size,
                &mut needed,
                &mut returned,
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        if rc != 0 {
            break (buffer, returned as usize);
        }
        let err = unsafe { GetLastError() };
        if err == ERROR_INSUFFICIENT_BUFFER {
            size = needed.max(size * 2);
            continue;
        }
        unsafe { CloseServiceHandle(scm) };
        anyhow::bail!("EnumServicesStatusExW failed (error {err})");
    };

    let esize = std::mem::size_of::<ENUM_SERVICE_STATUS_PROCESSW>();
    let mut entries = Vec::with_capacity(count);
    for i in 0..count {
        let e =
            unsafe { &*(buffer.as_ptr().add(i * esize) as *const ENUM_SERVICE_STATUS_PROCESSW) };
        let name = wide_to_string(e.lpServiceName);
        let display_name = wide_to_string(e.lpDisplayName);
        let status = state_label(e.ServiceStatusProcess.dwCurrentState);
        let pid = e.ServiceStatusProcess.dwProcessId as i32;
        // 启动类型需单独查配置；权限不足等留空不阻断
        let start_type = query_start_type(scm, &name).unwrap_or_default();

        entries.push(SysServiceEntry {
            status: status.to_string(),
            pid,
            name,
            display_name,
            start_type,
            description: String::new(),
            enabled_state: String::new(),
            since_unix: 0,
            unit_file: String::new(),
        });
    }
    unsafe { CloseServiceHandle(scm) };
    Ok(entries)
}

/// 服务名 → 启动类型（auto/manual/disabled/boot/system）。
fn query_start_type(scm: SC_HANDLE, name: &str) -> Option<String> {
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let svc = unsafe { OpenServiceW(scm, wide.as_ptr(), SERVICE_QUERY_CONFIG) };
    if svc.is_null() {
        return None;
    }
    let mut needed: u32 = 0;
    // 首次传 null 拿所需缓冲区大小
    unsafe { QueryServiceConfigW(svc, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0 {
        unsafe { CloseServiceHandle(svc) };
        return None;
    }
    let mut buffer = vec![0u8; needed as usize];
    let ok =
        unsafe { QueryServiceConfigW(svc, buffer.as_mut_ptr() as *mut _, needed, &mut needed) };
    let result = if ok != 0 {
        let cfg = unsafe { &*(buffer.as_ptr() as *const QUERY_SERVICE_CONFIGW) };
        Some(
            match cfg.dwStartType {
                SERVICE_AUTO_START => "auto",
                SERVICE_DEMAND_START => "manual",
                SERVICE_DISABLED => "disabled",
                SERVICE_BOOT_START => "boot",
                SERVICE_SYSTEM_START => "system",
                _ => "",
            }
            .to_string(),
        )
    } else {
        None
    };
    unsafe { CloseServiceHandle(svc) };
    result
}

/// 启动 / 停止系统服务（restart = stop → start，见 [`service_restart`]）。
pub fn service_action(name: &str, action: &str) -> Result<(), String> {
    let scm = open_scm(SC_MANAGER_CONNECT).map_err(|e| e.to_string())?;
    let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let result = match action {
        "start" => {
            let svc = unsafe { OpenServiceW(scm, wide.as_ptr(), SERVICE_START) };
            if svc.is_null() {
                Err(win_err("OpenService"))
            } else {
                let ok = unsafe { StartServiceW(svc, 0, std::ptr::null()) };
                unsafe { CloseServiceHandle(svc) };
                if ok != 0 {
                    Ok(())
                } else {
                    Err(win_err("StartService"))
                }
            }
        }
        "stop" => {
            let svc = unsafe { OpenServiceW(scm, wide.as_ptr(), SERVICE_STOP) };
            if svc.is_null() {
                Err(win_err("OpenService"))
            } else {
                let mut status = unsafe { std::mem::zeroed::<SERVICE_STATUS>() };
                let ok = unsafe { ControlService(svc, SERVICE_CONTROL_STOP, &mut status) };
                if ok == 0 {
                    unsafe { CloseServiceHandle(svc) };
                    Err(win_err("ControlService(STOP)"))
                } else {
                    // 轮询等待真正 STOPPED（最多 10s）
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
                    loop {
                        if unsafe { QueryServiceStatus(svc, &mut status) } == 0 {
                            break;
                        }
                        if status.dwCurrentState == SERVICE_STOPPED {
                            break;
                        }
                        if std::time::Instant::now() > deadline {
                            break;
                        }
                        std::thread::sleep(std::time::Duration::from_millis(200));
                    }
                    unsafe { CloseServiceHandle(svc) };
                    Ok(())
                }
            }
        }
        other => return Err(format!("unknown action: {other}")),
    };
    unsafe { CloseServiceHandle(scm) };
    result
}

/// 停止 → 启动（restart 语义；stop 失败即未运行，不阻断）。
pub fn service_restart(name: &str) -> Result<(), String> {
    let _ = service_action(name, "stop");
    std::thread::sleep(std::time::Duration::from_millis(500));
    service_action(name, "start")
}

fn open_scm(access: u32) -> anyhow::Result<SC_HANDLE> {
    let scm = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), access) };
    if scm.is_null() {
        anyhow::bail!("OpenSCManagerW failed: {}", win_err_string());
    }
    Ok(scm)
}

fn win_err_string() -> String {
    format!("error {}", unsafe { GetLastError() })
}

fn win_err(api: &str) -> String {
    format!("{api} failed ({})", win_err_string())
}

fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    unsafe {
        let mut len = 0usize;
        while *p.add(len) != 0 {
            len += 1;
        }
        String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
    }
}

fn state_label(state: u32) -> &'static str {
    match state {
        SERVICE_RUNNING => "running",
        SERVICE_STOPPED => "stopped",
        2 => "start_pending",
        3 => "stop_pending",
        5 => "continue_pending",
        6 => "pause_pending",
        7 => "paused",
        _ => "unknown",
    }
}

// ---------------------------------------------------------------------------
// 磁盘驱动器
// ---------------------------------------------------------------------------

/// 枚举逻辑驱动器：`(名称如 "C:\\", 类型标签)`。
pub fn list_drives() -> Vec<(String, &'static str)> {
    let bitmask = unsafe { GetLogicalDrives() };
    if bitmask == 0 {
        return Vec::new();
    }
    let mut drives = Vec::new();
    for i in 0..26u32 {
        if bitmask & (1 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            let name = format!("{letter}:\\");
            let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            let label = match kind {
                3 => "fixed",
                2 => "removable",
                4 => "network",
                5 => "cdrom",
                6 => "ramdisk",
                _ => "unknown",
            };
            drives.push((name, label));
        }
    }
    drives
}

// ---------------------------------------------------------------------------
// 网络适配器 + 连接表
// ---------------------------------------------------------------------------

/// 全量网络适配器（GetAdaptersAddressesW）：名称 / 单播地址 / MAC / 状态 / 网关 / 类型。
pub fn list_adapters() -> Vec<(String, Vec<String>, String, String, String, String)> {
    // GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_INCLUDE_ALL_INTERFACES
    const FLAGS: u32 = 0x0001 | 0x0100;

    let mut size: u32 = 16 * 1024;
    let mut buffer;
    loop {
        buffer = vec![0u8; size as usize];
        let rc = unsafe {
            GetAdaptersAddresses(
                0, // AF_UNSPEC
                FLAGS,
                std::ptr::null_mut(),
                buffer.as_mut_ptr() as *mut _,
                &mut size,
            )
        };
        if rc == ERROR_BUFFER_OVERFLOW {
            continue; // size 已更新，重新分配
        }
        if rc == NO_ERROR {
            break;
        }
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut cursor = buffer.as_ptr()
        as *const windows_sys::Win32::NetworkManagement::IpHelper::IP_ADAPTER_ADDRESSES_LH;
    while !cursor.is_null() {
        let adapter = unsafe { &*cursor };
        let name = wide_to_string(adapter.FriendlyName);
        let status = if adapter.OperStatus == IfOperStatusUp {
            "up"
        } else {
            "down"
        };
        let mac = if adapter.PhysicalAddressLength > 0 {
            adapter.PhysicalAddress[..adapter.PhysicalAddressLength as usize]
                .iter()
                .map(|b| format!("{b:02X}"))
                .collect::<Vec<_>>()
                .join("-")
        } else {
            String::new()
        };
        let kind = if_type_label(adapter.IfType);

        let mut addrs = Vec::new();
        let mut gw = String::new();
        let mut u = adapter.FirstUnicastAddress;
        while !u.is_null() {
            let sa = unsafe { (*u).Address.lpSockaddr };
            if let Some(ip) = sockaddr_to_ip(sa) {
                addrs.push(ip);
            }
            u = unsafe { (*u).Next };
        }
        let mut g = adapter.FirstGatewayAddress;
        while !g.is_null() {
            let sa = unsafe { (*g).Address.lpSockaddr };
            if gw.is_empty() {
                gw = sockaddr_to_ip(sa).unwrap_or_default();
            }
            g = unsafe { (*g).Next };
        }

        out.push((name, addrs, mac, status.to_string(), gw, kind.to_string()));
        cursor = adapter.Next as *const _;
    }
    out
}

fn sockaddr_to_ip(sa: *const windows_sys::Win32::Networking::WinSock::SOCKADDR) -> Option<String> {
    use windows_sys::Win32::Networking::WinSock::{AF_INET, AF_INET6, SOCKADDR_IN, SOCKADDR_IN6};
    if sa.is_null() {
        return None;
    }
    let family = unsafe { (*sa).sa_family };
    if family == AF_INET {
        let sin = sa as *const SOCKADDR_IN;
        let b = unsafe { (*sin).sin_addr.S_un.S_un_b };
        Some(Ipv4Addr::new(b.s_b1, b.s_b2, b.s_b3, b.s_b4).to_string())
    } else if family == AF_INET6 {
        let sin6 = sa as *const SOCKADDR_IN6;
        let bytes = unsafe { (*sin6).sin6_addr.u.Byte };
        Some(Ipv6Addr::from(bytes).to_string())
    } else {
        None
    }
}

fn if_type_label(t: u32) -> &'static str {
    match t {
        6 => "ethernet",
        71 => "wifi",
        24 => "loopback",
        53 | 131 => "tunnel",
        23 => "ppp",
        _ => "other",
    }
}

/// TCP/UDP 连接表（含归属 PID），pid→进程名由调用方补齐。
pub fn list_connections() -> Vec<NetConnection> {
    let mut conns = tcp_table();
    conns.extend(udp_table());
    conns
}

/// 内存中的网络字节序 u32 → IPv4 点分串。
fn ipv4_str(addr: u32) -> String {
    let b = addr.to_ne_bytes();
    Ipv4Addr::new(b[0], b[1], b[2], b[3]).to_string()
}

/// 网络字节序端口字段（低 16 位）→ 主机序端口号。
fn port_str(field: u32) -> String {
    u16::from_be((field & 0xFFFF) as u16).to_string()
}

fn tcp_table() -> Vec<NetConnection> {
    let mut out = tcp_table_family(windows_sys::Win32::Networking::WinSock::AF_INET as u32);
    out.extend(tcp_table_family(
        windows_sys::Win32::Networking::WinSock::AF_INET6 as u32,
    ));
    out
}

fn tcp_table_family(family: u32) -> Vec<NetConnection> {
    const TCP_TABLE_OWNER_PID_ALL:
        windows_sys::Win32::NetworkManagement::IpHelper::TCP_TABLE_CLASS = 4;
    let mut size: u32 = 0;
    let rc = unsafe {
        GetExtendedTcpTable(
            std::ptr::null_mut(),
            &mut size,
            0,
            family,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc != ERROR_INSUFFICIENT_BUFFER || size == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0u8; size as usize];
    let rc = unsafe {
        GetExtendedTcpTable(
            buffer.as_mut_ptr() as *mut _,
            &mut size,
            0,
            family,
            TCP_TABLE_OWNER_PID_ALL,
            0,
        )
    };
    if rc != NO_ERROR {
        return Vec::new();
    }

    let is_v6 = family == windows_sys::Win32::Networking::WinSock::AF_INET6 as u32;
    let ptr = buffer.as_ptr();
    let count = unsafe { *(ptr as *const u32) } as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        // v4 行 24B：state(0) local(4) lport(8) remote(12) rport(16) pid(20)
        // v6 行 56B：local(16) lport(16) remote(20) rport(36) state(40) pid(44)
        let (local, remote, state, pid) = unsafe {
            if is_v6 {
                let base = ptr.add(4).add(i * 56);
                let read_u32 = |off: usize| {
                    u32::from_le_bytes(
                        std::slice::from_raw_parts(base.add(off), 4)
                            .try_into()
                            .unwrap(),
                    )
                };
                let mut ip = [0u8; 16];
                ip.copy_from_slice(std::slice::from_raw_parts(base, 16));
                let local = format!("[{}]:{}", Ipv6Addr::from(ip), port_str(read_u32(16)));
                let mut rip = [0u8; 16];
                rip.copy_from_slice(std::slice::from_raw_parts(base.add(20), 16));
                let remote = format!("[{}]:{}", Ipv6Addr::from(rip), port_str(read_u32(36)));
                (local, remote, read_u32(40), read_u32(44))
            } else {
                let base = ptr.add(4).add(i * 24);
                let read_u32 = |off: usize| {
                    u32::from_le_bytes(
                        std::slice::from_raw_parts(base.add(off), 4)
                            .try_into()
                            .unwrap(),
                    )
                };
                let local = format!("{}:{}", ipv4_str(read_u32(4)), port_str(read_u32(8)));
                let remote = format!("{}:{}", ipv4_str(read_u32(12)), port_str(read_u32(16)));
                (local, remote, read_u32(0), read_u32(20))
            }
        };
        out.push(NetConnection {
            protocol: "tcp".into(),
            local,
            remote,
            state: tcp_state_label(state),
            pid: pid as i32,
            process_name: String::new(),
        });
    }
    out
}

fn udp_table() -> Vec<NetConnection> {
    const UDP_TABLE_OWNER_PID: windows_sys::Win32::NetworkManagement::IpHelper::UDP_TABLE_CLASS = 1;
    let mut out = Vec::new();
    for family in [
        windows_sys::Win32::Networking::WinSock::AF_INET as u32,
        windows_sys::Win32::Networking::WinSock::AF_INET6 as u32,
    ] {
        let is_v6 = family == windows_sys::Win32::Networking::WinSock::AF_INET6 as u32;
        let mut size: u32 = 0;
        let rc = unsafe {
            GetExtendedUdpTable(
                std::ptr::null_mut(),
                &mut size,
                0,
                family,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };
        if rc != ERROR_INSUFFICIENT_BUFFER || size == 0 {
            continue;
        }
        let mut buffer = vec![0u8; size as usize];
        let rc = unsafe {
            GetExtendedUdpTable(
                buffer.as_mut_ptr() as *mut _,
                &mut size,
                0,
                family,
                UDP_TABLE_OWNER_PID,
                0,
            )
        };
        if rc != NO_ERROR {
            continue;
        }
        let ptr = buffer.as_ptr();
        let count = unsafe { *(ptr as *const u32) } as usize;
        for i in 0..count {
            // v4 行 12B：addr(0) port(4) pid(8)；v6 行 24B：addr(16) port(16) pid(20)
            let (local, pid) = unsafe {
                if is_v6 {
                    let base = ptr.add(4).add(i * 24);
                    let read_u32 = |off: usize| {
                        u32::from_le_bytes(
                            std::slice::from_raw_parts(base.add(off), 4)
                                .try_into()
                                .unwrap(),
                        )
                    };
                    let mut ip = [0u8; 16];
                    ip.copy_from_slice(std::slice::from_raw_parts(base, 16));
                    (
                        format!("[{}]:{}", Ipv6Addr::from(ip), port_str(read_u32(16))),
                        read_u32(20),
                    )
                } else {
                    let base = ptr.add(4).add(i * 12);
                    let read_u32 = |off: usize| {
                        u32::from_le_bytes(
                            std::slice::from_raw_parts(base.add(off), 4)
                                .try_into()
                                .unwrap(),
                        )
                    };
                    (
                        format!("{}:{}", ipv4_str(read_u32(0)), port_str(read_u32(4))),
                        read_u32(8),
                    )
                }
            };
            out.push(NetConnection {
                protocol: "udp".into(),
                local,
                remote: "*:*".into(),
                state: String::new(),
                pid: pid as i32,
                process_name: String::new(),
            });
        }
    }
    out
}

fn tcp_state_label(state: u32) -> String {
    let label = match state {
        2 => "LISTEN",
        3 => "SYN_SENT",
        4 => "SYN_RCVD",
        5 => "ESTABLISHED",
        6 => "FIN_WAIT1",
        7 => "FIN_WAIT2",
        8 => "CLOSE_WAIT",
        9 => "CLOSING",
        10 => "LAST_ACK",
        11 => "TIME_WAIT",
        12 => "DELETE_TCB",
        1 => "CLOSED",
        _ => return String::new(),
    };
    label.to_string()
}

/// 进程绝对路径兜底：sysinfo 拿不到时用 QueryFullProcessImageNameW 再试一次。
pub fn process_image_path(pid: u32) -> Option<String> {
    const BUF: u32 = 1024;
    let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if handle.is_null() {
        return None;
    }
    let mut buf = [0u16; BUF as usize];
    let mut len = BUF;
    let ok = unsafe {
        QueryFullProcessImageNameW(handle, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
    };
    unsafe { CloseHandle(handle) };
    if ok == 0 || len == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}
