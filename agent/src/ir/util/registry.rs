//! 注册表：读（值 / 单值 / 子键 / CLSID → 服务器路径 / 数据解码）与写
//! （原始读写、删除、Autoruns 禁用/恢复）。
//!
//! 自 `ir/util.rs` 拆出（G7）。原分节：「注册表」+「注册表写操作（供 actions.rs 禁用/启用/删除）」。

use crate::ir::regcodec::decode_reg_data;
use crate::ir::text::expand_env;
use windows_sys::Win32::System::Registry::HKEY;

pub fn hkey_local() -> HKEY {
    windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE
}

/// 枚举某键下的全部值（REG_SZ/EXPAND_SZ 解码为字符串，DWORD 解码为十进制，
/// MULTI_SZ 以换行连接；其余显示 binary(NB)）。
pub fn reg_values(hive: HKEY, path: &str) -> Vec<(String, String)> {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegEnumValueW, RegOpenKeyExW};
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(hive, wpath.as_ptr(), 0, 0x20019, &mut hkey) } != 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    unsafe {
        for i in 0..2048u32 {
            let mut name = [0u16; 512];
            let mut name_len = name.len() as u32;
            let mut vtype = 0u32;
            let mut data = [0u8; 4096];
            let mut data_len = data.len() as u32;
            let rc = RegEnumValueW(
                hkey,
                i,
                name.as_mut_ptr(),
                &mut name_len,
                std::ptr::null(),
                &mut vtype,
                data.as_mut_ptr(),
                &mut data_len,
            );
            if rc != 0 {
                break;
            }
            let name_s = String::from_utf16_lossy(&name[..name_len as usize]);
            out.push((name_s, decode_reg_data(vtype, &data[..data_len as usize])));
        }
        RegCloseKey(hkey);
    }
    out
}

/// 读取单值（含默认值 ""；路径不存在返回 None）。
pub fn reg_get_value(hive: HKEY, path: &str, name: &str) -> Option<String> {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW};
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(hive, wpath.as_ptr(), 0, 0x20019, &mut hkey) } != 0 {
        return None;
    }
    let mut data = [0u8; 4096];
    let mut data_len = data.len() as u32;
    let mut vtype = 0u32;
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            wname.as_ptr(),
            std::ptr::null(),
            &mut vtype,
            data.as_mut_ptr(),
            &mut data_len,
        )
    };
    unsafe { RegCloseKey(hkey) };
    if rc != 0 {
        return None;
    }
    Some(decode_reg_data(vtype, &data[..data_len as usize]))
}

/// 枚举子键名。
pub fn reg_subkeys(hive: HKEY, path: &str) -> Vec<String> {
    use windows_sys::Win32::System::Registry::{
        KEY_READ, RegCloseKey, RegEnumKeyExW, RegOpenKeyExW,
    };
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(hive, wpath.as_ptr(), 0, KEY_READ, &mut hkey) } != 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    unsafe {
        for i in 0..4096u32 {
            let mut name = [0u16; 512];
            let mut name_len = name.len() as u32;
            let rc = RegEnumKeyExW(
                hkey,
                i,
                name.as_mut_ptr(),
                &mut name_len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if rc != 0 {
                break;
            }
            out.push(String::from_utf16_lossy(&name[..name_len as usize]));
        }
        RegCloseKey(hkey);
    }
    out
}

/// 解析 CLSID → 服务器 DLL/EXE 路径（InprocServer32 优先，其次 LocalServer32）。
pub fn clsid_server(clsid: &str) -> Option<String> {
    const ROOTS: &[&str] = &[
        r"SOFTWARE\Classes\CLSID",
        r"SOFTWARE\Classes\Wow6432Node\CLSID",
    ];
    for root in ROOTS {
        for sub in ["InprocServer32", "LocalServer32"] {
            let path = format!("{root}\\{clsid}\\{sub}");
            if let Some(v) = reg_get_value(hkey_local(), &path, "") {
                let v = expand_env(v.trim().trim_matches('"'));
                if !v.is_empty() {
                    return Some(v);
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// 注册表写操作（供 actions.rs 禁用/启用/删除）
// ---------------------------------------------------------------------------

pub const AUTORUNS_DISABLED_SUBKEY: &str = "AutorunsDisabled";

/// 解析 hive 字符串：HKLM / HKCU / HKU:<用户SID>。
pub fn hive_from_str(s: &str) -> Option<HKEY> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, HKEY_USERS};
    // 常量本身安全，unsafe 仅在解引用句柄时需要
    match s {
        "HKLM" => Some(HKEY_LOCAL_MACHINE),
        "HKCU" => Some(HKEY_CURRENT_USER),
        "HKU" => Some(HKEY_USERS),
        rest => rest.strip_prefix("HKU:").map(|_| HKEY_USERS),
    }
}

/// 读取值的原始数据 + 类型（容量上限 64KB）。
pub fn reg_get_raw(hive: HKEY, path: &str, name: &str) -> Option<(u32, Vec<u8>)> {
    use windows_sys::Win32::System::Registry::{RegCloseKey, RegOpenKeyExW, RegQueryValueExW};
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(hive, wpath.as_ptr(), 0, 0x20019, &mut hkey) } != 0 {
        return None;
    }
    let mut data = vec![0u8; 65536];
    let mut data_len = data.len() as u32;
    let mut vtype = 0u32;
    let rc = unsafe {
        RegQueryValueExW(
            hkey,
            wname.as_ptr(),
            std::ptr::null(),
            &mut vtype,
            data.as_mut_ptr(),
            &mut data_len,
        )
    };
    unsafe { RegCloseKey(hkey) };
    if rc != 0 {
        return None;
    }
    data.truncate(data_len as usize);
    Some((vtype, data))
}

/// 写入值的原始数据 + 类型。
pub fn reg_set_raw(
    hive: HKEY,
    path: &str,
    name: &str,
    vtype: u32,
    data: &[u8],
) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        KEY_WRITE, RegCloseKey, RegCreateKeyExW, RegSetValueExW,
    };
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = std::ptr::null_mut();
    let rc = unsafe {
        RegCreateKeyExW(
            hive,
            wpath.as_ptr(),
            0,
            std::ptr::null(),
            0,
            KEY_WRITE,
            std::ptr::null(),
            &mut hkey,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return Err(format!("打开/创建键失败: {path} (rc={rc})"));
    }
    let rc = unsafe {
        RegSetValueExW(
            hkey,
            wname.as_ptr(),
            0,
            vtype,
            data.as_ptr(),
            data.len() as u32,
        )
    };
    unsafe { RegCloseKey(hkey) };
    if rc != 0 {
        return Err(format!("写值失败: {name} (rc={rc})"));
    }
    Ok(())
}

/// 删除值（返回是否删除成功；值不存在视为成功）。
pub fn reg_delete_value(hive: HKEY, path: &str, name: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{
        KEY_SET_VALUE, RegCloseKey, RegDeleteValueW, RegOpenKeyExW,
    };
    let wpath: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
    let mut hkey = std::ptr::null_mut();
    if unsafe { RegOpenKeyExW(hive, wpath.as_ptr(), 0, KEY_SET_VALUE, &mut hkey) } != 0 {
        return Ok(()); // 键都不存在 = 目标状态已达成
    }
    let rc = unsafe { RegDeleteValueW(hkey, wname.as_ptr()) };
    unsafe { RegCloseKey(hkey) };
    if rc == 0 || rc == 2 {
        Ok(())
    } else {
        Err(format!("删值失败: {name} (rc={rc})"))
    }
}

/// 把父键下的一个值移动到 `<父键>\AutorunsDisabled` 子键（Autoruns 禁用机制）。
pub fn reg_move_value_to_disabled(hive: HKEY, subkey: &str, name: &str) -> Result<(), String> {
    let disabled_path = format!("{subkey}\\{AUTORUNS_DISABLED_SUBKEY}");
    let (vtype, data) =
        reg_get_raw(hive, subkey, name).ok_or_else(|| format!("值不存在: {subkey}\\{name}"))?;
    reg_set_raw(hive, &disabled_path, name, vtype, &data)?;
    reg_delete_value(hive, subkey, name)
}

/// 把值从 `<父键>\AutorunsDisabled` 子键移回父键（Autoruns 启用机制）。
pub fn reg_restore_value_from_disabled(hive: HKEY, subkey: &str, name: &str) -> Result<(), String> {
    let disabled_path = format!("{subkey}\\{AUTORUNS_DISABLED_SUBKEY}");
    let (vtype, data) = reg_get_raw(hive, &disabled_path, name)
        .ok_or_else(|| format!("禁用区不存在该值: {disabled_path}\\{name}"))?;
    reg_set_raw(hive, subkey, name, vtype, &data)?;
    reg_delete_value(hive, &disabled_path, name)
}
