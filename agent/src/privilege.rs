//! 当前进程权限检测：Windows 管理员 / Unix root。

/// 是否以提权身份运行（Windows = Administrators 组成员；Unix = uid 0）。
pub fn is_elevated() -> bool {
    #[cfg(windows)]
    {
        is_elevated_windows()
    }
    #[cfg(not(windows))]
    {
        is_elevated_unix()
    }
}

#[cfg(windows)]
fn is_elevated_windows() -> bool {
    use windows_sys::Win32::Foundation::BOOL;
    use windows_sys::Win32::Security::{
        AllocateAndInitializeSid, CheckTokenMembership, FreeSid, PSID,
        SECURITY_NT_AUTHORITY,
    };
    const SECURITY_BUILTIN_DOMAIN_RID: u32 = 0x20; // 32
    const DOMAIN_ALIAS_RID_ADMINS: u32 = 0x220; // 544
    unsafe {
        let authority = SECURITY_NT_AUTHORITY;
        let mut admins: PSID = std::ptr::null_mut();
        // S-1-5-32-544 = BUILTIN\Administrators
        if AllocateAndInitializeSid(
            &authority,
            2,
            SECURITY_BUILTIN_DOMAIN_RID,
            DOMAIN_ALIAS_RID_ADMINS,
            0,
            0,
            0,
            0,
            0,
            0,
            &mut admins,
        ) == 0
        {
            return false;
        }
        let mut member: BOOL = 0;
        let ok = CheckTokenMembership(std::ptr::null_mut(), admins, &mut member);
        if !admins.is_null() {
            FreeSid(admins);
        }
        ok != 0 && member != 0
    }
}

#[cfg(unix)]
fn is_elevated_unix() -> bool {
    // /proc/self/status 的 Uid 行第一列 = 有效 uid
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines().find(|l| l.starts_with("Uid:")).and_then(|l| {
                l.split_whitespace().nth(1).and_then(|uid| uid.parse::<u32>().ok())
            })
        })
        .map(|uid| uid == 0)
        .unwrap_or(false)
}
