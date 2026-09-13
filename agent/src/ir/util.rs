//! IR 公共工具：注册表读取、命令行解析、文件签名校验（Authenticode + 目录签名）、
//! 厂商信息（版本资源）与扫描器上下文。

use helm_proto::pb::IrFinding;
use std::collections::HashMap;
use windows_sys::Win32::System::Registry::HKEY;

pub const MAX_FINDINGS: usize = 4000;

/// 扫描器上下文：汇总发现 + 文件信息（厂商/签名）缓存（同一文件只校验一次）
/// + 条目去重（不同扫描器扫到同一持久化点时只保留一条）。
pub struct Scanner {
    findings: Vec<IrFinding>,
    cache: HashMap<String, (Option<String>, String)>,
    seen: std::collections::HashSet<String>,
}

/// 一条待推送的条目（op_key 供禁用/启用/删除操作寻址，格式见 actions.rs）。
pub struct Entry {
    pub category: String,
    pub name: String,
    pub detail: String,
    pub severity: String,
    pub path: Option<String>,
    pub desc: Option<String>,
    pub op_key: Option<String>,
    pub disabled: bool,
}

impl Entry {
    pub fn new(
        category: &str,
        name: &str,
        detail: impl Into<String>,
        severity: &str,
        path: Option<String>,
    ) -> Self {
        Self {
            category: category.to_string(),
            name: name.to_string(),
            detail: detail.into(),
            severity: severity.to_string(),
            path,
            desc: None,
            op_key: None,
            disabled: false,
        }
    }

    pub fn desc(mut self, d: impl Into<String>) -> Self {
        self.desc = Some(d.into());
        self
    }

    pub fn op_key(mut self, k: impl Into<String>) -> Self {
        self.op_key = Some(k.into());
        self
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }
}

impl Scanner {
    pub fn new() -> Self {
        Self {
            findings: Vec::new(),
            cache: HashMap::new(),
            seen: std::collections::HashSet::new(),
        }
    }

    /// 条目去重键（同类别同名同详情视为同一条）。
    fn dedup_key(category: &str, name: &str, detail: &str) -> String {
        format!("{category}\u{1f}{name}\u{1f}{detail}")
    }

    /// 文件信息（厂商, 签名状态）。path 为 None 时返回 (None, "")。
    pub fn file_info(&mut self, path: Option<&str>) -> (Option<String>, String) {
        match path {
            Some(p) => self
                .cache
                .entry(p.to_string())
                // 厂商优先证书主体（Autoruns Publisher 语义），回退版本资源 CompanyName
                .or_insert_with(|| {
                    (
                        file_signer(p).or_else(|| file_publisher(p)),
                        file_sign_state(p),
                    )
                })
                .clone(),
            None => (None, String::new()),
        }
    }

    /// 推送一条原始发现（账户/事件/文件类，无文件富化）。
    pub fn push_raw(
        &mut self,
        category: &str,
        name: &str,
        detail: impl Into<String>,
        severity: &str,
    ) {
        let detail = detail.into();
        let key = Self::dedup_key(category, name, &detail);
        if !self.seen.insert(key) {
            return;
        }
        self.findings.push(IrFinding {
            category: category.to_string(),
            name: name.to_string(),
            detail,
            severity: severity.to_string(),
            path: None,
            publisher: None,
            sign_state: None,
            desc: None,
            op_key: None,
            disabled: None,
            mtime: None,
            ts_unix: None,
        });
    }

    /// 推送一条带时间戳的原始发现（时间线用）。
    pub fn push_raw_ts(
        &mut self,
        category: &str,
        name: &str,
        detail: impl Into<String>,
        severity: &str,
        ts_unix: i64,
    ) {
        let detail = detail.into();
        let key = Self::dedup_key(category, name, &detail);
        if !self.seen.insert(key) {
            return;
        }
        self.findings.push(IrFinding {
            category: category.to_string(),
            name: name.to_string(),
            detail,
            severity: severity.to_string(),
            path: None,
            publisher: None,
            sign_state: None,
            desc: None,
            op_key: None,
            disabled: None,
            mtime: None,
            ts_unix: (ts_unix > 0).then_some(ts_unix as u64),
        });
    }

    /// 推送一条带文件信息的条目：自动补全 publisher / sign_state / mtime，
    /// 并按路径位置与签名状态修正严重级别（显式传 critical 不降级）。
    pub fn push_entry(&mut self, e: Entry) {
        let key = Self::dedup_key(&e.category, &e.name, &e.detail);
        if !self.seen.insert(key) {
            return;
        }
        let (publisher, sign) = self.file_info(e.path.as_deref());
        let severity = if e.severity == "critical" {
            e.severity
        } else {
            severity_for(e.path.as_deref(), &sign).to_string()
        };
        self.findings.push(IrFinding {
            category: e.category,
            name: e.name,
            detail: e.detail,
            severity,
            ts_unix: None,
            mtime: e.path.as_deref().and_then(file_mtime),
            path: e.path,
            publisher,
            sign_state: (!sign.is_empty()).then_some(sign),
            desc: e.desc,
            op_key: e.op_key,
            disabled: e.disabled.then_some(true),
        });
    }

    /// 便捷包装（无操作寻址的条目）。
    pub fn push(
        &mut self,
        category: &str,
        name: &str,
        detail: impl Into<String>,
        severity: &str,
        path: Option<String>,
        desc: Option<String>,
    ) {
        let mut e = Entry::new(category, name, detail, severity, path);
        e.desc = desc;
        self.push_entry(e);
    }

    pub fn done(self) -> Vec<IrFinding> {
        self.findings
    }
}

/// 文件最后修改时间（unix 秒字符串）。
pub fn file_mtime(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let t = meta.modified().ok()?;
    let secs = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(secs.to_string())
}

// ---------------------------------------------------------------------------
// 严重级别启发式（配合签名状态）
// ---------------------------------------------------------------------------

/// 可疑目录：Temp/Public/PerfLogs/ProgramData 根等常见落地位置。
pub fn is_suspicious_dir(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.contains("\\temp\\")
        || p.ends_with("\\temp")
        || p.contains("\\public\\")
        || p.contains("\\perflogs\\")
        || (p.contains("\\programdata\\") && !p.contains("\\microsoft\\"))
}

/// 系统位置：Windows 目录或 Program Files。
pub fn is_system_dir(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.starts_with("c:\\windows\\")
        || p.starts_with("c:\\program files\\")
        || p.starts_with("c:\\program files (x86)\\")
}

/// 综合路径位置与签名状态的严重级别。
pub fn severity_for(path: Option<&str>, sign: &str) -> &'static str {
    if let Some(p) = path {
        if !std::path::Path::new(p).exists() {
            return "warn"; // 文件缺失：条目悬空或文件被杀软删除
        }
        if is_suspicious_dir(p) {
            return "critical";
        }
        if sign == "invalid" {
            return "critical";
        }
        if sign == "unsigned" && !is_system_dir(p) {
            return "warn";
        }
    }
    "info"
}

// ---------------------------------------------------------------------------
// 命令行 → 可执行文件路径
// ---------------------------------------------------------------------------

/// 展开 %SystemRoot% 等常见环境变量（免 ExpandEnvironmentStringsW 的额外 FFI）。
pub fn expand_env(path: &str) -> String {
    let mut p = path.to_string();
    for (var, val) in [
        ("%SystemRoot%", "C:\\Windows"),
        ("%systemroot%", "C:\\Windows"),
        ("%windir%", "C:\\Windows"),
        ("%WINDIR%", "C:\\Windows"),
        ("%SystemDrive%", "C:"),
        ("%systemdrive%", "C:"),
        ("%ProgramFiles%", "C:\\Program Files"),
        ("%ProgramFiles(x86)%", "C:\\Program Files (x86)"),
        ("%ProgramData%", "C:\\ProgramData"),
    ] {
        if p.contains(var) {
            p = p.replace(var, val);
        }
    }
    if let Ok(appdata) = std::env::var("APPDATA")
        && p.contains("%APPDATA%")
    {
        p = p.replace("%APPDATA%", &appdata);
        p = p.replace("%appdata%", &appdata);
    }
    p
}

fn file_exists(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

/// 从命令行提取可执行/DLL 路径：
/// `"C:\a b\x.exe" -arg`、`C:\a\x.exe -arg`、`rundll32.exe foo.dll,Entry`。
/// 返回前经 resolve_pe_path 归一化相对/内核风格路径。
pub fn extract_exe(cmdline: &str) -> Option<String> {
    let s = cmdline.trim();
    if s.is_empty() {
        return None;
    }
    let (first, rest) = if let Some(r) = s.strip_prefix('"') {
        match r.find('"') {
            Some(i) => (r[..i].to_string(), r[i + 1..].trim().to_string()),
            None => (s.replace('"', ""), String::new()),
        }
    } else {
        match s.find(' ') {
            Some(i) => (s[..i].to_string(), s[i..].trim().to_string()),
            None => (s.to_string(), String::new()),
        }
    };

    // 加载器（rundll32/regsvr32/脚本宿主等）：实际载荷是第一个参数
    let base = first
        .to_ascii_lowercase()
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("")
        .to_string();
    const LOADERS: &[&str] = &[
        "rundll32.exe",
        "regsvr32.exe",
        "mshta.exe",
        "wscript.exe",
        "cscript.exe",
        "cmd.exe",
        "powershell.exe",
        "pwsh.exe",
    ];
    if LOADERS.contains(&base.as_str()) && !rest.is_empty() {
        let rest_trim = rest.trim_start_matches('@');
        let tok = if let Some(r2) = rest_trim.strip_prefix('"') {
            r2.split('"').next().unwrap_or("").to_string()
        } else {
            rest_trim.split([' ', ',']).next().unwrap_or("").to_string()
        };
        let tok = resolve_pe_path(&expand_env(&tok));
        if file_exists(&tok) {
            return Some(tok);
        }
        let tok2 = resolve_pe_path(tok.split(',').next().unwrap_or(""));
        if file_exists(&tok2) {
            return Some(tok2);
        }
    }

    let p = resolve_pe_path(&expand_env(&first));
    if file_exists(&p) {
        return Some(p);
    }
    // 路径含空格但未加引号：逐 token 拼接尝试
    let mut acc = first.clone();
    for tok in rest.split(' ') {
        if tok.is_empty() {
            continue;
        }
        acc.push(' ');
        acc.push_str(tok);
        let cand = resolve_pe_path(&expand_env(acc.trim()));
        if file_exists(&cand) {
            return Some(cand);
        }
    }
    if p.contains('\\') || p.contains('/') {
        return Some(p); // 保底：返回原样供展示
    }
    None
}

/// 归一化注册表中的可执行路径：展开 `\SystemRoot`、`\??\`、裸 DLL 名等
/// 内核/服务风格路径为绝对路径（找不到则以最可能的候选返回）。
pub fn resolve_pe_path(raw: &str) -> String {
    let raw = raw.trim().trim_matches('"');
    if raw.is_empty() {
        return String::new();
    }
    let p = raw
        .replace("\\SystemRoot\\", "C:\\Windows\\")
        .replace("\\SystemRoot", "C:\\Windows");
    let p = p.strip_prefix("\\??\\").map(|s| s.to_string()).unwrap_or(p);
    let p = expand_env(&p);
    let lower = p.to_ascii_lowercase();
    if lower.starts_with("system32\\") {
        return format!(r"C:\Windows\{p}");
    }
    if p.starts_with('\\') && !p.starts_with("\\\\") {
        return format!("C:{p}");
    }
    if !p.contains(':') && !p.starts_with('\\') {
        // 裸文件名或相对路径：按 System32 → drivers → Windows 顺序探测
        for prefix in [
            r"C:\Windows\System32\",
            r"C:\Windows\System32\drivers\",
            r"C:\Windows\",
        ] {
            let cand = format!("{prefix}{p}");
            if file_exists(&cand) {
                return cand;
            }
        }
    }
    p
}

// ---------------------------------------------------------------------------
// 文件厂商（版本资源）与签名校验
// ---------------------------------------------------------------------------

/// 读取 PE 版本资源 CompanyName。
pub fn file_publisher(path: &str) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut handle = 0u32;
        let size = GetFileVersionInfoSizeW(wide.as_ptr(), &mut handle);
        if size == 0 {
            return None;
        }
        let mut buf = vec![0u8; size as usize];
        if GetFileVersionInfoW(wide.as_ptr(), 0, size, buf.as_mut_ptr().cast()) == 0 {
            return None;
        }
        // 语言/代码页翻译表
        let q: Vec<u16> = r"\VarFileInfo\Translation"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut off: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if VerQueryValueW(buf.as_ptr().cast(), q.as_ptr(), &mut off, &mut len) == 0
            || off.is_null()
            || len < 4
        {
            return None;
        }
        let trans = off as *const u16;
        let (lang, cp) = (*trans, *trans.add(1));
        let key = format!(r"\StringFileInfo\{lang:04x}{cp:04x}\CompanyName");
        let keyw: Vec<u16> = key.encode_utf16().chain(std::iter::once(0)).collect();
        let mut val: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut vlen = 0u32;
        if VerQueryValueW(buf.as_ptr().cast(), keyw.as_ptr(), &mut val, &mut vlen) == 0
            || val.is_null()
        {
            return None;
        }
        let n = (vlen as usize).saturating_sub(1).min(512);
        let s = String::from_utf16_lossy(std::slice::from_raw_parts(val as *const u16, n));
        let s = s.trim().trim_end_matches('\0').trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

/// Authenticode 校验（含目录签名）：
/// verified = 有效签名；unsigned = 无签名；invalid = 签名无效/被吊销；unknown = 文件不可读/校验失败。
pub fn file_sign_state(path: &str) -> String {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let rc = wintrust_file(wide.as_ptr());
    match rc as u32 {
        0 => "verified".to_string(),
        x if x == windows_sys::Win32::Foundation::TRUST_E_NOSIGNATURE as u32 => {
            // 无嵌入签名 → 尝试目录签名（Windows 系统文件大多走 catalog）
            let is_driver = path.to_ascii_lowercase().ends_with(".sys");
            if catalog_verify(wide.as_ptr(), is_driver) {
                "verified".to_string()
            } else {
                "unsigned".to_string()
            }
        }
        x if x == windows_sys::Win32::Foundation::TRUST_E_SUBJECT_FORM_UNKNOWN as u32 => {
            "unsigned".to_string()
        }
        0x8009_2003 => "unknown".to_string(), // CRYPT_E_FILE_ERROR：文件不可读/不存在
        0x8007_0010 | 0x8007_0002 => "unknown".to_string(), // ERROR_ 文件级错误
        _ if rc < 0 => "invalid".to_string(),
        _ => "unknown".to_string(),
    }
}

fn wintrust_data() -> windows_sys::Win32::Security::WinTrust::WINTRUST_DATA {
    use windows_sys::Win32::Security::WinTrust::*;
    WINTRUST_DATA {
        cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
        pPolicyCallbackData: std::ptr::null_mut(),
        pSIPClientData: std::ptr::null_mut(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_NONE,
        dwUnionChoice: 0,
        Anonymous: WINTRUST_DATA_0 {
            pFile: std::ptr::null_mut(),
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        hWVTStateData: std::ptr::null_mut(),
        pwszURLReference: std::ptr::null_mut(),
        dwProvFlags: 0,
        dwUIContext: WTD_UICONTEXT_EXECUTE,
        pSignatureSettings: std::ptr::null_mut(),
    }
}

/// 嵌入式签名校验，返回 WinVerifyTrust 原始返回码。
fn wintrust_file(pcwsz: *const u16) -> i32 {
    use windows_sys::Win32::Security::WinTrust::*;
    let mut file_info = WINTRUST_FILE_INFO {
        cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
        pcwszFilePath: pcwsz,
        hFile: std::ptr::null_mut(),
        pgKnownSubject: std::ptr::null_mut(),
    };
    let mut td = wintrust_data();
    td.dwUnionChoice = WTD_CHOICE_FILE;
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    unsafe {
        td.Anonymous.pFile = &mut file_info;
        let rc = WinVerifyTrust(std::ptr::null_mut(), &mut action, &td as *const _ as *mut _);
        td.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(std::ptr::null_mut(), &mut action, &td as *const _ as *mut _);
        rc
    }
}

/// 目录签名校验（Windows 系统文件无嵌入签名，靠 catalog 溯源）。
/// 实测 Win10/11 上 DRIVER_ACTION_VERIFY 对用户态与内核目录都能命中，
/// GENERIC_VERIFY_V2 只覆盖部分内核目录——两种策略依次尝试。
fn catalog_verify(pcwsz: *const u16, driver_first: bool) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Cryptography::Catalog::*;
    use windows_sys::Win32::Security::WinTrust::*;
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, OPEN_EXISTING,
    };

    let driver = windows_sys::Win32::Security::WinTrust::DRIVER_ACTION_VERIFY;
    let generic = windows_sys::Win32::Security::WinTrust::WINTRUST_ACTION_GENERIC_VERIFY_V2;
    let policies: &[windows_sys::core::GUID] = if driver_first {
        &[driver, generic]
    } else {
        &[generic, driver]
    };

    unsafe {
        // Win10+ 目录多为 SHA256；老目录 SHA1，逐一尝试
        for policy in policies {
            let mut action = *policy;
            let mut td = wintrust_data();
            td.dwUnionChoice = WTD_CHOICE_CATALOG;
            for alg in ["SHA256", "SHA1"] {
                let alg_w: Vec<u16> = alg.encode_utf16().chain(std::iter::once(0)).collect();
                let mut admin: isize = 0;
                let ok = CryptCATAdminAcquireContext2(
                    &mut admin,
                    policy,
                    alg_w.as_ptr(),
                    std::ptr::null(),
                    0,
                );
                if ok == 0 && CryptCATAdminAcquireContext(&mut admin, policy, 0) == 0 {
                    continue; // 该策略/算法组合不可用，换下一个
                }
                let hfile = CreateFileW(
                    pcwsz,
                    windows_sys::Win32::Foundation::GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_DELETE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                );
                let mut verified = false;
                if hfile != INVALID_HANDLE_VALUE {
                    let mut hash = [0u8; 128];
                    let mut hash_len = hash.len() as u32;
                    if CryptCATAdminCalcHashFromFileHandle2(
                        admin,
                        hfile,
                        &mut hash_len,
                        hash.as_mut_ptr(),
                        0,
                    ) != 0
                        && hash_len > 0
                    {
                        let hash = &hash[..hash_len as usize];
                        let member_tag: Vec<u16> = hash
                            .iter()
                            .map(|b| format!("{b:02X}"))
                            .collect::<String>()
                            .encode_utf16()
                            .chain(std::iter::once(0))
                            .collect();
                        let mut prev: isize = 0;
                        let mut hcat = CryptCATAdminEnumCatalogFromHash(
                            admin,
                            hash.as_ptr(),
                            hash_len,
                            0,
                            &mut prev,
                        );
                        while hcat != 0 {
                            let mut cat_info = CATALOG_INFO {
                                cbStruct: std::mem::size_of::<CATALOG_INFO>() as u32,
                                wszCatalogFile: [0; 260],
                            };
                            if CryptCATCatalogInfoFromContext(hcat, &mut cat_info, 0) != 0 {
                                let cat_path: Vec<u16> = {
                                    let end = cat_info
                                        .wszCatalogFile
                                        .iter()
                                        .position(|&c| c == 0)
                                        .unwrap_or(260);
                                    cat_info.wszCatalogFile[..end].to_vec()
                                };
                                let cat_path: Vec<u16> =
                                    cat_path.into_iter().chain(std::iter::once(0)).collect();
                                let mut wci = WINTRUST_CATALOG_INFO {
                                    cbStruct: std::mem::size_of::<WINTRUST_CATALOG_INFO>() as u32,
                                    dwCatalogVersion: 0,
                                    pcwszCatalogFilePath: cat_path.as_ptr(),
                                    pcwszMemberTag: member_tag.as_ptr(),
                                    pcwszMemberFilePath: pcwsz,
                                    hMemberFile: std::ptr::null_mut(),
                                    pbCalculatedFileHash: hash.as_ptr() as *mut u8,
                                    cbCalculatedFileHash: hash.len() as u32,
                                    pcCatalogContext: std::ptr::null_mut(),
                                    hCatAdmin: admin,
                                };
                                td.Anonymous.pCatalog = &mut wci;
                                let rc = WinVerifyTrust(
                                    std::ptr::null_mut(),
                                    &mut action,
                                    &td as *const _ as *mut _,
                                );
                                td.dwStateAction = WTD_STATEACTION_CLOSE;
                                let _ = WinVerifyTrust(
                                    std::ptr::null_mut(),
                                    &mut action,
                                    &td as *const _ as *mut _,
                                );
                                td.dwStateAction = WTD_STATEACTION_VERIFY;
                                if rc == 0 {
                                    verified = true;
                                    break;
                                }
                            }
                            prev = hcat;
                            hcat = CryptCATAdminEnumCatalogFromHash(
                                admin,
                                hash.as_ptr(),
                                hash_len,
                                0,
                                &mut prev,
                            );
                        }
                    }
                    CloseHandle(hfile);
                }
                CryptCATAdminReleaseContext(admin, 0);
                if verified {
                    return true;
                }
            }
        }
    }
    false
}

// ---------------------------------------------------------------------------
// 注册表
// ---------------------------------------------------------------------------

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

fn decode_reg_data(vtype: u32, data: &[u8]) -> String {
    match vtype {
        1 | 2 => {
            let u16s: Vec<u16> = data[..(data.len() / 2) * 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s)
                .trim_end_matches('\0')
                .to_string()
        }
        4 if data.len() >= 4 => {
            u32::from_le_bytes([data[0], data[1], data[2], data[3]]).to_string()
        }
        7 => {
            // REG_MULTI_SZ
            let u16s: Vec<u16> = data[..(data.len() / 2) * 2]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            u16s.split(|&c| c == 0)
                .filter(|s| !s.is_empty())
                .map(String::from_utf16_lossy)
                .collect::<Vec<_>>()
                .join("\n")
        }
        _ => format!("binary({}B)", data.len()),
    }
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
// 通用
// ---------------------------------------------------------------------------

/// 子进程命令（CREATE_NO_WINDOW，不闪控制台）。
pub fn child_cmd(program: &str) -> std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new(program);
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        cmd
    }
    #[cfg(not(windows))]
    {
        let _ = program;
        std::process::Command::new("cmd")
    }
}

/// 控制台输出解码（GBK → UTF-8）。
pub fn decode_console(bytes: &[u8]) -> String {
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    decoded.into_owned()
}

/// 单行 CSV 字段拆分（处理带引号、引号内逗号）。
pub fn split_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '"' {
            in_quotes = true;
        } else if c == ',' {
            fields.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
    }
    fields.push(cur);
    fields
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
// ---------------------------------------------------------------------------
// 证书主体（Authenticode 签名者名，Autoruns "Publisher" 列语义）
// ---------------------------------------------------------------------------

/// 取嵌入式签名的签名者主体名（如 "Microsoft Windows"）；无嵌入签名返回 None。
pub fn file_signer(path: &str) -> Option<String> {
    use windows_sys::Win32::Security::Cryptography::{
        CERT_CONTEXT, CERT_FIND_SUBJECT_CERT, CERT_NAME_SIMPLE_DISPLAY_TYPE,
        CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED, CERT_QUERY_FORMAT_FLAG_ALL,
        CERT_QUERY_OBJECT_FILE, CertCloseStore, CertFindCertificateInStore,
        CertFreeCertificateContext, CertGetNameStringW, CryptMsgClose, CryptMsgGetParam,
        CryptQueryObject, PKCS_7_ASN_ENCODING, X509_ASN_ENCODING,
    };
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut store: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut msg: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut ctx: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut enc_type = 0u32;
        let mut content_type = 0u32;
        let mut format_type = 0u32;
        let ok = CryptQueryObject(
            CERT_QUERY_OBJECT_FILE,
            wide.as_ptr().cast(),
            CERT_QUERY_CONTENT_FLAG_PKCS7_SIGNED_EMBED,
            CERT_QUERY_FORMAT_FLAG_ALL,
            0,
            &mut enc_type,
            &mut content_type,
            &mut format_type,
            &mut store,
            &mut msg,
            &mut ctx,
        );
        if ok == 0 {
            return None;
        }
        // 取签名者证书信息（CERT_INFO）
        let mut info_len = 0u32;
        let _ = CryptMsgGetParam(msg, 7u32, 0, std::ptr::null_mut(), &mut info_len); // CMSG_SIGNER_CERT_INFO_PARAM
        let mut buf = vec![0u8; info_len as usize];
        let got = CryptMsgGetParam(msg, 7u32, 0, buf.as_mut_ptr().cast(), &mut info_len); // CMSG_SIGNER_CERT_INFO_PARAM
        let mut name = String::new();
        if got != 0 {
            let cert_info = buf.as_ptr() as *const _;
            let cert_ctx: *const CERT_CONTEXT = std::ptr::null();
            let found = CertFindCertificateInStore(
                store,
                X509_ASN_ENCODING | PKCS_7_ASN_ENCODING,
                0,
                CERT_FIND_SUBJECT_CERT,
                cert_info,
                cert_ctx,
            );
            if !found.is_null() {
                let mut name_buf = [0u16; 512];
                let n = CertGetNameStringW(
                    found,
                    CERT_NAME_SIMPLE_DISPLAY_TYPE,
                    0,
                    std::ptr::null(),
                    name_buf.as_mut_ptr(),
                    name_buf.len() as u32,
                );
                if n > 1 {
                    name = String::from_utf16_lossy(&name_buf[..(n as usize - 1).min(511)]);
                }
                CertFreeCertificateContext(found);
            }
        }
        if !msg.is_null() {
            CryptMsgClose(msg);
        }
        if !store.is_null() {
            CertCloseStore(store, 0);
        }
        let s = name.trim().to_string();
        (!s.is_empty()).then_some(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn probe_sign_states() {
        let files = [
            r"C:\Windows\System32\alg.exe",
            r"C:\Windows\System32\msdtc.exe",
            r"C:\Windows\System32\msiexec.exe",
            r"C:\Windows\System32\svchost.exe",
            r"C:\Windows\System32\notepad.exe",
            r"C:\Windows\System32\drivers\1394ohci.sys",
        ];
        for f in files {
            let wide: Vec<u16> = f.encode_utf16().chain(std::iter::once(0)).collect();
            let rc = wintrust_file(wide.as_ptr());
            let is_driver = f.to_ascii_lowercase().ends_with(".sys");
            let cat = catalog_verify(wide.as_ptr(), is_driver);
            println!("{f}: embedded_rc={rc:#x} catalog_ok={cat}");
        }
    }
}
