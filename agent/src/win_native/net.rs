//! 网络适配器与连接表（GetAdaptersAddressesW / GetExtendedTcpTable / GetExtendedUdpTable）。
//!
//! 自 `win_native.rs` 拆出（G7：该文件生产段 614 行越界）。父模块保留服务（SCM）、磁盘与
//! 进程路径查询；本模块只处理网络面——**替代 netstat 解析**，连接行带归属 PID。
//!
//! 行布局（与 Win32 结构体一致的裸偏移，改结构体版本需同步）：
//! - TCP v4 24B：state(0) local(4) lport(8) remote(12) rport(16) pid(20)
//! - TCP v6 56B：local(16) lport(16) remote(20) rport(36) state(40) pid(44)
//! - UDP v4 12B：addr(0) port(4) pid(8)；UDP v6 24B：addr(16) port(16) pid(20)

use super::wide_to_string;
use crate::netfmt::{if_type_label, ipv4_str, port_str, tcp_state_label};
use helm_proto::pb::NetConnection;
use std::net::{Ipv4Addr, Ipv6Addr};
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_INSUFFICIENT_BUFFER, NO_ERROR};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GetExtendedTcpTable, GetExtendedUdpTable,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;

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

/// TCP/UDP 连接表（含归属 PID），pid→进程名由调用方补齐。
pub fn list_connections() -> Vec<NetConnection> {
    let mut conns = tcp_table();
    conns.extend(udp_table());
    conns
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
