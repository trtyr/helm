//! Windows 原生能力（内置能力层，对标 Process Hacker 的实现方式）：
//! - 服务：SCM 原生枚举/启停（替代 PowerShell/CIM，无外部进程、无编码问题）
//! - 网络适配器：GetAdaptersAddressesW（全量接口，含 MAC/网关/状态）——见 [`net`]
//! - 连接表：GetExtendedTcpTable / GetExtendedUdpTable（含归属 PID，替代 netstat 解析）——见 [`net`]
//! - 磁盘：GetLogicalDrives / GetDriveTypeW（驱动器枚举，文件管理根视图）
//! - 进程：QueryFullProcessImageNameW（绝对路径兜底查询）
//!
//! 仅 Windows 编译；其他平台调用方自行回退到命令行方案。
//!
//! **文件布局（G7 拆分，2026-09-20）**：网络面（适配器 + 连接表）拆到 [`net`]；本文件保留
//! 服务（SCM）、磁盘与进程路径查询。两个子模块条目都在此 `pub use` 再导出，调用方无感。

#![cfg(windows)]

use helm_proto::pb::SysServiceEntry;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
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

mod net;

pub use net::{list_adapters, list_connections};

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
        if err == windows_sys::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER {
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
    let _ = service_action(name, "stop"); // restart 语义：stop 失败即服务本未运行，不阻断后续 start
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
