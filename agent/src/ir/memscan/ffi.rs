//! memscan 的 Win32 FFI 与字符串提取层：提权、字节缓冲 → 字符串、UTF-8 安全截断。
//!
//! 自 `memscan.rs` 拆出（G7：该文件生产段 469 行越界）。扫描编排（进程枚举、区域遍历、
//! 批次推送）仍在父模块；本层不依赖扫描状态，可独立审读。

/// 启用 SeDebugPrivilege（管理员令牌默认持有但未激活——不显式启用则无法读 SYSTEM 进程内存）。
pub(super) fn enable_debug_privilege() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let name: Vec<u16> = "SeDebugPrivilege"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut luid = windows_sys::Win32::Foundation::LUID {
            LowPart: 0,
            HighPart: 0,
        };
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return false;
        }
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [windows_sys::Win32::Security::LUID_AND_ATTRIBUTES {
                Luid: windows_sys::Win32::Foundation::LUID {
                    LowPart: 0,
                    HighPart: 0,
                },
                Attributes: 0,
            }],
        };
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        let ok =
            AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
        CloseHandle(token);
        ok != 0
    }
}

/// 提取 ASCII 可打印字符串（长度 ≥ min_len），按顺序返回。
pub fn extract_ascii_strings(buf: &[u8], min_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &b) in buf.iter().enumerate() {
        if (0x20..0x7F).contains(&b) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take()
            && i - s >= min_len
        {
            out.push(String::from_utf8_lossy(&buf[s..i]).into_owned());
        }
    }
    if let Some(s) = start
        && buf.len() - s >= min_len
    {
        out.push(String::from_utf8_lossy(&buf[s..]).into_owned());
    }
    out
}

/// 提取 UTF-16LE 字符串（长度 ≥ min_len 字符）。
pub fn extract_utf16_strings(buf: &[u8], min_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut start: Option<usize> = None;
    for (i, &u) in units.iter().enumerate() {
        let printable = (0x20..0x7F).contains(&u) || (0x4E00..=0x9FFF).contains(&u);
        if printable {
            start.get_or_insert(i);
        } else if let Some(s) = start.take()
            && i - s >= min_len
        {
            out.push(String::from_utf16_lossy(&units[s..i]));
        }
    }
    if let Some(s) = start
        && units.len() - s >= min_len
    {
        out.push(String::from_utf16_lossy(&units[s..]));
    }
    out
}

/// UTF-8 安全截断：不切断多字节字符。
pub(super) fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}
