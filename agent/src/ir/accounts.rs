//! 账户审计：隐藏账户 / 克隆账户（低 RID）/ 弱口令策略 / SpecialAccounts 隐藏 / 管理员组。
//!
//! 结构（G11，2026-09-21）：`scan` 只做阶段编排，四类发现各自落在具名函数里
//! （枚举 / 判读 / 注册表 / 组），便于单独阅读与增补规则。

use super::util::{Scanner, hkey_local, reg_values};
use std::collections::HashMap;

pub fn scan(sc: &mut Scanner) {
    // 阶段 1：全量用户（level 23 含 SID）+ 账户标志（level 1）
    let users = enum_users_with_sid();
    let flag_map = enum_user_flags();

    // 阶段 2：隐藏账户 / 克隆账户 / 标志异常 判读
    report_user_anomalies(sc, &users, &flag_map);

    // 阶段 3：SpecialAccounts 注册表隐藏
    report_special_accounts(sc);

    // 阶段 4：Administrators 组成员
    report_admin_members(sc);
}

/// 阶段 1a：枚举全量本地用户（level 23，含 SID），返回 `(用户名, RID)`。
///
/// `NetUserEnum` 返回 `ERROR_MORE_DATA(234)` 表示还有下一页，需继续循环并释放缓冲区。
fn enum_users_with_sid() -> Vec<(String, u32)> {
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        NetApiBufferFree, NetUserEnum, USER_INFO_23,
    };

    let mut users: Vec<(String, u32)> = Vec::new();
    let mut buf: *mut u8 = std::ptr::null_mut();
    let mut read = 0u32;
    let mut total = 0u32;
    let mut resume = 0u32;
    loop {
        let rc = unsafe {
            NetUserEnum(
                std::ptr::null(),
                23,
                0,
                &mut buf,
                0xFFFF,
                &mut read,
                &mut total,
                &mut resume,
            )
        };
        if rc != 0 && rc != 234 {
            break;
        }
        let rows = buf as *const USER_INFO_23;
        for i in 0..read as usize {
            let r = unsafe { &*rows.add(i) };
            users.push((pwstr(r.usri23_name), sid_rid(r.usri23_user_sid)));
        }
        if !buf.is_null() {
            unsafe { NetApiBufferFree(buf.cast()) };
        }
        if rc != 234 {
            break;
        }
    }
    users
}

/// 阶段 1b：枚举账户标志（level 1），返回 `用户名 → 「已禁用/密码永不过期/无需密码」`。
///
/// 只保留有标志的用户（无标志的不入表），与调用点原先的 `if !marks.is_empty()` 一致。
fn enum_user_flags() -> HashMap<String, String> {
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        NetApiBufferFree, NetUserEnum, USER_INFO_1,
    };

    let mut flag_map = HashMap::new();
    let mut buf: *mut u8 = std::ptr::null_mut();
    let mut read = 0u32;
    let mut total = 0u32;
    let mut resume = 0u32;
    loop {
        let rc = unsafe {
            NetUserEnum(
                std::ptr::null(),
                1,
                0,
                &mut buf,
                0xFFFF,
                &mut read,
                &mut total,
                &mut resume,
            )
        };
        if rc != 0 && rc != 234 {
            break;
        }
        let rows = buf as *const USER_INFO_1;
        for i in 0..read as usize {
            let r = unsafe { &*rows.add(i) };
            let name = pwstr(r.usri1_name);
            // 位判读抽到 `ir::text::user_flag_marks`（P006 P0-8）：那里是平台无关纯函数，
            // macOS 上也能单测——正是为了钉住「0x0100（临时域账户）≠ 密码永不过期」这个错。
            let marks = super::text::user_flag_marks(r.usri1_flags);
            if !marks.is_empty() {
                flag_map.insert(name, marks.join("/"));
            }
        }
        if !buf.is_null() {
            unsafe { NetApiBufferFree(buf.cast()) };
        }
        if rc != 234 {
            break;
        }
    }
    flag_map
}

/// 阶段 2：隐藏账户（`$` 结尾）/ 克隆账户（低 RID）/ 标志异常 判读。
///
/// RID 500/501 是内置 Administrator/Guest，0 是无效值——三者不参与「低 RID」判据，
/// 否则每次扫描都会把内置管理员当克隆账户报出来。
fn report_user_anomalies(
    sc: &mut Scanner,
    users: &[(String, u32)],
    flag_map: &HashMap<String, String>,
) {
    for (name, rid) in users {
        let display = flag_map.get(name).cloned().unwrap_or_default();
        if name.ends_with('$') {
            sc.push_raw(
                "账户",
                name,
                format!("$ 隐藏账户（net user 不可见，RID {rid}）"),
                "critical",
            );
        } else if *rid < 1000 && *rid != 500 && *rid != 501 && *rid != 0 {
            sc.push_raw(
                "账户",
                name,
                format!("低 RID {rid}——疑似克隆账户"),
                "critical",
            );
        } else if !display.is_empty() {
            sc.push_raw("账户", name, format!("{}（{}）", name, display), "info");
        }
    }
}

/// 阶段 3：SpecialAccounts\UserList 注册表显式隐藏的账户（值为 0 或空）。
fn report_special_accounts(sc: &mut Scanner) {
    let sp = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Winlogon\SpecialAccounts\UserList";
    for (name, val) in reg_values(hkey_local(), sp) {
        if val == "0" || val.is_empty() {
            sc.push_raw(
                "账户",
                &name,
                "SpecialAccounts\\UserList 显式隐藏（登录界面不可见）".to_string(),
                "critical",
            );
        }
    }
}

/// 阶段 4：Administrators 组成员（SID → 名称）。
fn report_admin_members(sc: &mut Scanner) {
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        LOCALGROUP_MEMBERS_INFO_0, NetApiBufferFree, NetLocalGroupGetMembers,
    };

    let mut buf: *mut u8 = std::ptr::null_mut();
    let mut read = 0u32;
    let mut total = 0u32;
    let group: Vec<u16> = "Administrators"
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let rc = unsafe {
        NetLocalGroupGetMembers(
            std::ptr::null(),
            group.as_ptr(),
            0,
            &mut buf,
            0xFFFF,
            &mut read,
            &mut total,
            std::ptr::null_mut(),
        )
    };
    if rc != 0 {
        return;
    }
    let rows = buf as *const LOCALGROUP_MEMBERS_INFO_0;
    let mut names = Vec::new();
    for i in 0..read as usize {
        let r = unsafe { &*rows.add(i) };
        names.push(sid_to_name(r.lgrmi0_sid));
    }
    sc.push_raw("账户", "Administrators 组成员", names.join("、"), "info");
    if !buf.is_null() {
        unsafe { NetApiBufferFree(buf.cast()) };
    }
}

fn pwstr(p: *const u16) -> String {
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

fn sid_rid(psid: windows_sys::Win32::Security::PSID) -> u32 {
    use windows_sys::Win32::Security::{GetSidSubAuthority, GetSidSubAuthorityCount};
    if psid.is_null() {
        return 0;
    }
    unsafe {
        let count = *GetSidSubAuthorityCount(psid);
        if count == 0 {
            return 0;
        }
        *GetSidSubAuthority(psid, (count - 1) as u32)
    }
}

fn sid_to_name(psid: windows_sys::Win32::Security::PSID) -> String {
    use windows_sys::Win32::Security::{LookupAccountSidW, SidTypeUnknown};
    unsafe {
        let mut name = [0u16; 256];
        let mut name_len = name.len() as u32;
        let mut domain = [0u16; 256];
        let mut domain_len = domain.len() as u32;
        let mut sid_type = SidTypeUnknown;
        let ok = LookupAccountSidW(
            std::ptr::null(),
            psid,
            name.as_mut_ptr(),
            &mut name_len,
            domain.as_mut_ptr(),
            &mut domain_len,
            &mut sid_type,
        );
        if ok == 0 {
            return "?".to_string();
        }
        String::from_utf16_lossy(&name[..name_len as usize])
    }
}
