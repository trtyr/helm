//! 启动项操作：禁用 / 启用 / 删除（对标 Autoruns 的 AutorunsDisabled 机制）。
//!
//! op_key 操作地址格式（\u{1f} 分隔）：
//! - `reg\u{1f}<hive>\u{1f}<subkey>\u{1f}<value>` — 注册表值；hive ∈ HKLM | HKCU | HKU:<SID>
//!   禁用 = 值移入 `<subkey>\AutorunsDisabled`；启用 = 移回；删除 = 删值
//! - `file\u{1f}<绝对路径>` — 启动文件夹文件；禁用 = 移入同级 AutorunsDisabled 子目录
//! - `svc\u{1f}<服务名>` — 服务/驱动；禁用 = 原始 Start 存入 AutorunsDisabled 值 + Start=4
//! - `task\u{1f}<任务名>` — 计划任务；schtasks /change | /delete

use super::util::{
    AUTORUNS_DISABLED_SUBKEY, child_cmd, hive_from_str, reg_delete_value, reg_get_raw,
    reg_get_value, reg_move_value_to_disabled, reg_restore_value_from_disabled,
    reg_set_raw, hkey_local,
};
use helm_proto::pb::{AgentMessage, AutorunsActionResult, agent_message};
use windows_sys::Win32::System::Registry::HKEY;

/// 执行启动项操作并回包。
pub fn autoruns_action(request_id: &str, action: &str, op_key: &str) -> AgentMessage {
    let result = run_action(action, op_key);
    let (ok, error) = match result {
        Ok(()) => (true, None),
        Err(e) => {
            tracing::warn!(action, op_key, error = %e, "autoruns_action failed");
            (false, Some(e))
        }
    };
    AgentMessage {
        kind: Some(agent_message::Kind::AutorunsActionResult(AutorunsActionResult {
            request_id: request_id.to_string(),
            ok,
            error,
        })),
    }
}

fn run_action(action: &str, op_key: &str) -> Result<(), String> {
    let parts: Vec<&str> = op_key.split('\u{1f}').collect();
    match (action, parts.as_slice()) {
        ("disable" | "enable" | "delete", ["reg", hive, subkey, value]) => {
            reg_action(action, hive, subkey, value)
        }
        ("disable" | "enable" | "delete", ["file", path]) => file_action(action, path),
        ("disable" | "enable" | "delete", ["svc", name]) => svc_action(action, name),
        ("disable", ["task", name]) => task_schtask("/change", "/disable", name),
        ("enable", ["task", name]) => task_schtask("/change", "/enable", name),
        ("delete", ["task", name]) => task_schtask("/delete", "/f", name),
        _ => Err(format!("不支持的操作目标: {op_key}")),
    }
}

// ---------------------------------------------------------------------------
// 注册表值（AutorunsDisabled 子键移动）
// ---------------------------------------------------------------------------

fn reg_action(action: &str, hive_tag: &str, subkey: &str, value: &str) -> Result<(), String> {
    let hive: HKEY = hive_from_str(hive_tag).ok_or_else(|| format!("未知 hive: {hive_tag}"))?;
    let subkey = if hive_tag.starts_with("HKU:") {
        // HKU:<SID> → 实际子键 = <SID>\<rest>（hive_from_str 对 HKU: 返回 HKEY_USERS）
        let sid = hive_tag.trim_start_matches("HKU:");
        format!("{sid}\\{subkey}")
    } else {
        subkey.to_string()
    };

    match action {
        "disable" => {
            if subkey.ends_with(AUTORUNS_DISABLED_SUBKEY) {
                return Err("该条目已处于禁用状态".into());
            }
            reg_move_value_to_disabled(hive, &subkey, value)
        }
        "enable" => {
            if let Some(parent) = subkey.strip_suffix(&format!("\\{AUTORUNS_DISABLED_SUBKEY}")) {
                // op_key 已指向禁用区：移回父键
                let (vtype, data) = reg_get_raw(hive, &subkey, value)
                    .ok_or_else(|| format!("禁用区不存在该值: {subkey}\\{value}"))?;
                reg_set_raw(hive, parent, value, vtype, &data)?;
                reg_delete_value(hive, &subkey, value)
            } else {
                // 常规位置有值 = 已启用；若禁用区也有则清掉禁用区副本
                if reg_get_raw(hive, &subkey, value).is_some() {
                    Ok(())
                } else {
                    reg_restore_value_from_disabled(hive, &subkey, value)
                }
            }
        }
        "delete" => reg_delete_value(hive, &subkey, value),
        _ => Err(format!("未知操作: {action}")),
    }
}

// ---------------------------------------------------------------------------
// 启动文件夹文件（AutorunsDisabled 子目录移动）
// ---------------------------------------------------------------------------

fn file_action(action: &str, path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    match action {
        "disable" => {
            let Some(parent) = p.parent() else { return Err("无父目录".into()) };
            if p.parent().unwrap().file_name().map(|n| n == AUTORUNS_DISABLED_SUBKEY).unwrap_or(false) {
                return Err("该条目已处于禁用状态".into());
            }
            let target_dir = parent.join(AUTORUNS_DISABLED_SUBKEY);
            std::fs::create_dir_all(&target_dir).map_err(|e| format!("建禁用目录失败: {e}"))?;
            let target = target_dir.join(p.file_name().ok_or("无文件名")?);
            if target.exists() {
                return Err(format!("禁用区已存在同名文件: {}", target.display()));
            }
            std::fs::rename(p, &target).map_err(|e| format!("移动失败: {e}"))
        }
        "enable" => {
            let Some(parent) = p.parent() else { return Err("无父目录".into()) };
            let Some(real_parent) = parent.parent() else { return Err("无祖父目录".into()) };
            let target = real_parent.join(p.file_name().ok_or("无文件名")?);
            if target.exists() {
                return Err(format!("目标已存在: {}", target.display()));
            }
            std::fs::rename(p, &target).map_err(|e| format!("移动失败: {e}"))
        }
        "delete" => {
            if p.is_dir() {
                std::fs::remove_dir_all(p).map_err(|e| format!("删除失败: {e}"))
            } else {
                std::fs::remove_file(p).map_err(|e| format!("删除失败: {e}"))
            }
        }
        _ => Err(format!("未知操作: {action}")),
    }
}

// ---------------------------------------------------------------------------
// 服务 / 驱动（Start ↔ AutorunsDisabled 值互换；删除走 SCM DeleteService）
// ---------------------------------------------------------------------------

fn svc_key(name: &str) -> String {
    format!(r"SYSTEM\CurrentControlSet\Services\{name}")
}

fn svc_action(action: &str, name: &str) -> Result<(), String> {
    let hive = hkey_local();
    let key = svc_key(name);
    match action {
        "disable" => {
            let start = reg_get_value(hive, &key, "Start")
                .and_then(|v| v.trim().parse::<u32>().ok())
                .unwrap_or(3);
            if start == 4 && reg_get_value(hive, &key, AUTORUNS_DISABLED_SUBKEY).is_some() {
                return Err("该服务已处于禁用状态".into());
            }
            // 保存原始 Start（DWORD）
            reg_set_raw(hive, &key, AUTORUNS_DISABLED_SUBKEY, 4, &start.to_le_bytes())?;
            reg_set_raw(hive, &key, "Start", 4, &4u32.to_le_bytes())
        }
        "enable" => {
            let original = reg_get_value(hive, &key, AUTORUNS_DISABLED_SUBKEY)
                .and_then(|s| s.trim().parse::<u32>().ok());
            let restored = original.unwrap_or(3); // 无记录则恢复为手动
            reg_set_raw(hive, &key, "Start", 4, &restored.to_le_bytes())?;
            reg_delete_value(hive, &key, AUTORUNS_DISABLED_SUBKEY)
        }
        "delete" => svc_delete(name),
        _ => Err(format!("未知操作: {action}")),
    }
}

/// SCM 删除服务（标记删除；运行中的进程退出/重启后消失）。
fn svc_delete(name: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, DeleteService, OpenSCManagerW, OpenServiceW, SC_MANAGER_ALL_ACCESS,
        SERVICE_ALL_ACCESS,
    };
    unsafe {
        let wname: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_ALL_ACCESS);
        if scm.is_null() {
            return Err("OpenSCManager 失败（需管理员）".into());
        }
        let svc = OpenServiceW(scm, wname.as_ptr(), SERVICE_ALL_ACCESS);
        if svc.is_null() {
            CloseServiceHandle(scm);
            return Err(format!("打开服务失败: {name}"));
        }
        let ok = DeleteService(svc);
        CloseServiceHandle(svc);
        CloseServiceHandle(scm);
        if ok == 0 {
            return Err(format!("DeleteService 失败: {name}"));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// 计划任务（schtasks）
// ---------------------------------------------------------------------------

fn task_schtask(sub: &str, flag: &str, name: &str) -> Result<(), String> {
    let out = child_cmd("schtasks")
        .args([sub, flag, "/tn", name])
        .output()
        .map_err(|e| format!("schtasks 启动失败: {e}"))?;
    if out.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&out.stderr);
    Err(format!("schtasks {sub} 失败: {}", stderr.trim()))
}
