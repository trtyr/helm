//! 账户审计：隐藏账户 / 克隆账户（低 RID）/ 弱口令策略 / SpecialAccounts 隐藏 / 管理员组。

use super::util::{Scanner, hkey_local, reg_values};
use std::collections::HashMap;

pub fn scan(sc: &mut Scanner) {
    use windows_sys::Win32::NetworkManagement::NetManagement::{
        NetApiBufferFree, NetLocalGroupGetMembers, NetUserEnum, USER_INFO_1, USER_INFO_23,
    };

    unsafe {
        // 全量用户（level 23 含 SID）
        let mut users: Vec<(String, u32)> = Vec::new();
        let mut buf: *mut u8 = std::ptr::null_mut();
        let mut read = 0u32;
        let mut total = 0u32;
        let mut resume = 0u32;
        loop {
            let rc = NetUserEnum(
                std::ptr::null(),
                23,
                0,
                &mut buf,
                0xFFFF,
                &mut read,
                &mut total,
                &mut resume,
            );
            if rc != 0 && rc != 234 {
                break;
            }
            let rows = buf as *const USER_INFO_23;
            for i in 0..read as usize {
                let r = &*rows.add(i);
                let name = pwstr(r.usri23_name);
                let rid = sid_rid(r.usri23_user_sid);
                users.push((name, rid));
            }
            if !buf.is_null() {
                NetApiBufferFree(buf.cast());
            }
            if rc != 234 {
                break;
            }
        }

        // 账户标志（level 1 补充）
        let mut flag_map = HashMap::new();
        let mut b1: *mut u8 = std::ptr::null_mut();
        let mut r1 = 0u32;
        let mut t1 = 0u32;
        let mut res1 = 0u32;
        loop {
            let rc = NetUserEnum(
                std::ptr::null(),
                1,
                0,
                &mut b1,
                0xFFFF,
                &mut r1,
                &mut t1,
                &mut res1,
            );
            if rc != 0 && rc != 234 {
                break;
            }
            let rows = b1 as *const USER_INFO_1;
            for i in 0..r1 as usize {
                let r = &*rows.add(i);
                let name = pwstr(r.usri1_name);
                let mut marks = Vec::new();
                if r.usri1_flags & 0x0002 != 0 {
                    marks.push("已禁用");
                }
                if r.usri1_flags & 0x0100 != 0 {
                    marks.push("密码永不过期");
                }
                if r.usri1_flags & 0x0020 != 0 {
                    marks.push("无需密码");
                }
                if !marks.is_empty() {
                    flag_map.insert(name, marks.join("/"));
                }
            }
            if !b1.is_null() {
                NetApiBufferFree(b1.cast());
            }
            if rc != 234 {
                break;
            }
        }

        for (name, rid) in &users {
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

        // SpecialAccounts 注册表隐藏
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

        // Administrators 组成员
        let mut mbuf: *mut u8 = std::ptr::null_mut();
        let mut mread = 0u32;
        let mut mtotal = 0u32;
        let group: Vec<u16> = "Administrators"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let rc = NetLocalGroupGetMembers(
            std::ptr::null(),
            group.as_ptr(),
            0,
            &mut mbuf,
            0xFFFF,
            &mut mread,
            &mut mtotal,
            std::ptr::null_mut(),
        );
        if rc == 0 {
            let rows =
                mbuf as *const windows_sys::Win32::NetworkManagement::NetManagement::LOCALGROUP_MEMBERS_INFO_0;
            let mut names = Vec::new();
            for i in 0..mread as usize {
                let r = &*rows.add(i);
                names.push(sid_to_name(r.lgrmi0_sid));
            }
            sc.push_raw("账户", "Administrators 组成员", names.join("、"), "info");
            if !mbuf.is_null() {
                NetApiBufferFree(mbuf.cast());
            }
        }
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
