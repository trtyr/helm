//! 厂商（版本资源）与签名校验：版本资源 CompanyName、Authenticode 嵌入签名、目录签名（catalog）、
//! 以及签名者主体名（Autoruns "Publisher" 列语义）。
//!
//! 自 `ir/util.rs` 拆出（G7）。原分节：「文件厂商（版本资源）与签名校验」+「证书主体」。

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
        // 关闭状态动作：清理调用，失败不影响结论（rc 已取得）
        let _ = WinVerifyTrust(std::ptr::null_mut(), &mut action, &td as *const _ as *mut _);
        rc
    }
}

/// 目录签名校验（Windows 系统文件无嵌入签名，靠 catalog 溯源）。
/// 实测 Win10/11 上 DRIVER_ACTION_VERIFY 对用户态与内核目录都能命中，
/// GENERIC_VERIFY_V2 只覆盖部分内核目录——两种策略依次尝试。
///
/// **G11 拆分（2026-09-21）**：原为 141 行单块（策略 × 算法双层循环 + 上下文获取 +
/// 文件哈希 + 目录枚举 + 逐项目校验全在一处）。现拆为——`catalog_policies`（策略顺序）/
/// `verify_by_policy`（单策略单算法）/ `file_catalog_hash`（文件哈希）/
/// `member_tag`（哈希 → 成员标签）/ `verify_catalog_entries` + `verify_catalog_entry`（目录项校验）。
fn catalog_verify(pcwsz: *const u16, driver_first: bool) -> bool {
    for policy in catalog_policies(driver_first) {
        // Win10+ 目录多为 SHA256；老目录 SHA1，逐一尝试
        for alg in ["SHA256", "SHA1"] {
            if verify_by_policy(pcwsz, &policy, alg) {
                return true;
            }
        }
    }
    false
}

/// 策略尝试顺序：按调用方提示决定先试驱动目录还是通用目录。
fn catalog_policies(driver_first: bool) -> [windows_sys::core::GUID; 2] {
    let driver = windows_sys::Win32::Security::WinTrust::DRIVER_ACTION_VERIFY;
    let generic = windows_sys::Win32::Security::WinTrust::WINTRUST_ACTION_GENERIC_VERIFY_V2;
    if driver_first {
        [driver, generic]
    } else {
        [generic, driver]
    }
}

/// 单策略 + 单哈希算法尝试：取目录管理员上下文 → 算文件哈希 → 枚举目录项校验。
/// 策略/算法组合不可用时返回 false（调用方换下一个组合）。
fn verify_by_policy(pcwsz: *const u16, policy: &windows_sys::core::GUID, alg: &str) -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Cryptography::Catalog::{
        CryptCATAdminAcquireContext, CryptCATAdminAcquireContext2, CryptCATAdminReleaseContext,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, OPEN_EXISTING,
    };

    unsafe {
        let alg_w: Vec<u16> = alg.encode_utf16().chain(std::iter::once(0)).collect();
        let mut admin: isize = 0;
        let ok =
            CryptCATAdminAcquireContext2(&mut admin, policy, alg_w.as_ptr(), std::ptr::null(), 0);
        if ok == 0 && CryptCATAdminAcquireContext(&mut admin, policy, 0) == 0 {
            return false; // 该策略/算法组合不可用，换下一个
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
            if let Some(hash) = file_catalog_hash(admin, hfile) {
                verified = verify_catalog_entries(admin, &hash, pcwsz, policy);
            }
            CloseHandle(hfile);
        }
        CryptCATAdminReleaseContext(admin, 0);
        verified
    }
}

/// 计算文件在目录中的哈希（`CryptCATAdminCalcHashFromFileHandle2`）；失败或空哈希返回 `None`。
fn file_catalog_hash(
    admin: isize,
    hfile: windows_sys::Win32::Foundation::HANDLE,
) -> Option<Vec<u8>> {
    use windows_sys::Win32::Security::Cryptography::Catalog::CryptCATAdminCalcHashFromFileHandle2;

    unsafe {
        let mut hash = [0u8; 128];
        let mut hash_len = hash.len() as u32;
        if CryptCATAdminCalcHashFromFileHandle2(admin, hfile, &mut hash_len, hash.as_mut_ptr(), 0)
            == 0
            || hash_len == 0
        {
            return None;
        }
        Some(hash[..hash_len as usize].to_vec())
    }
}

/// 文件哈希 → 目录成员标签（逐字节大写十六进制 + NUL 结尾的 UTF-16）。
fn member_tag(hash: &[u8]) -> Vec<u16> {
    hash.iter()
        .map(|b| format!("{b:02X}"))
        .collect::<String>()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect()
}

/// 逐个目录项校验（同一哈希可能命中多个目录，任一通过即算通过）。
fn verify_catalog_entries(
    admin: isize,
    hash: &[u8],
    pcwsz: *const u16,
    policy: &windows_sys::core::GUID,
) -> bool {
    use windows_sys::Win32::Security::Cryptography::Catalog::CryptCATAdminEnumCatalogFromHash;

    unsafe {
        let tag = member_tag(hash);
        let mut prev: isize = 0;
        let mut hcat =
            CryptCATAdminEnumCatalogFromHash(admin, hash.as_ptr(), hash.len() as u32, 0, &mut prev);
        while hcat != 0 {
            if verify_catalog_entry(admin, hcat, pcwsz, &tag, hash, policy) {
                return true;
            }
            prev = hcat;
            hcat = CryptCATAdminEnumCatalogFromHash(
                admin,
                hash.as_ptr(),
                hash.len() as u32,
                0,
                &mut prev,
            );
        }
        false
    }
}

/// 单个目录项校验：构造 `WINTRUST_CATALOG_INFO` 走 `WinVerifyTrust`，`rc == 0` 即通过。
fn verify_catalog_entry(
    admin: isize,
    hcat: isize,
    pcwsz: *const u16,
    member_tag: &[u16],
    hash: &[u8],
    policy: &windows_sys::core::GUID,
) -> bool {
    use windows_sys::Win32::Security::Cryptography::Catalog::{
        CATALOG_INFO, CryptCATCatalogInfoFromContext,
    };
    use windows_sys::Win32::Security::WinTrust::{
        WINTRUST_CATALOG_INFO, WTD_CHOICE_CATALOG, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WinVerifyTrust,
    };

    unsafe {
        let mut cat_info = CATALOG_INFO {
            cbStruct: std::mem::size_of::<CATALOG_INFO>() as u32,
            wszCatalogFile: [0; 260],
        };
        if CryptCATCatalogInfoFromContext(hcat, &mut cat_info, 0) == 0 {
            return false;
        }
        let cat_path: Vec<u16> = {
            let end = cat_info
                .wszCatalogFile
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(260);
            cat_info.wszCatalogFile[..end].to_vec()
        };
        let cat_path: Vec<u16> = cat_path.into_iter().chain(std::iter::once(0)).collect();

        let mut action = *policy;
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
        let mut td = wintrust_data();
        td.dwUnionChoice = WTD_CHOICE_CATALOG;
        td.dwStateAction = WTD_STATEACTION_VERIFY;
        td.Anonymous.pCatalog = &mut wci;
        let rc = WinVerifyTrust(std::ptr::null_mut(), &mut action, &td as *const _ as *mut _);
        td.dwStateAction = WTD_STATEACTION_CLOSE;
        // 关闭状态动作：清理调用，失败不影响结论
        let _ = WinVerifyTrust(std::ptr::null_mut(), &mut action, &td as *const _ as *mut _);
        rc == 0
    }
}

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
        // 首次调用只取所需缓冲区大小（惯例探询），失败时下一行的 got!=0 判定已覆盖
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
