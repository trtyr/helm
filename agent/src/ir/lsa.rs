//! 认证与系统提供程序（Autoruns 思路中的 LSA / Provider 持久化）：
//! LSA 认证包、安全包、通知包、SecurityProviders、凭据提供程序、打印监视器、网络提供程序。
//! 这些 DLL 在登录/系统服务加载期运行——绕过杀软自启检测的经典落点。

use super::util::{Scanner, clsid_server, expand_env, hkey_local, reg_get_value, reg_subkeys, resolve_pe_path};

const LSA_ROOT: &str = r"SYSTEM\CurrentControlSet\Control\Lsa";
const CC_ROOT: &str = r"SYSTEM\CurrentControlSet\Control";

pub fn scan(sc: &mut Scanner) {
    lsa_packages(sc);
    security_providers(sc);
    credential_providers(sc);
    print_monitors(sc);
}

/// LSA 包：Authentication Packages / Notification Packages / Security Packages（DLL 名，位于 System32）。
fn lsa_packages(sc: &mut Scanner) {
    const PKG_KEYS: &[(&str, &str)] = &[
        (r"Authentication Packages", "认证包"),
        (r"Notification Packages", "通知包"),
        (r"Security Packages", "安全包"),
        (r"MSV1_0\Authentication Packages", "认证包(MSV1_0)"),
        (r"MSV1_0\Notification Packages", "通知包(MSV1_0)"),
    ];
    for (sub, label) in PKG_KEYS {
        if let Some(v) = reg_get_value(hkey_local(), &format!("{LSA_ROOT}\\{sub}"), "") {
            for dll in v.lines().filter(|s| !s.trim().is_empty()) {
                let name = dll.trim().to_ascii_lowercase();
                let path = if name.ends_with(".dll") {
                    format!(r"C:\Windows\System32\{name}")
                } else {
                    format!(r"C:\Windows\System32\{name}.dll")
                };
                sc.push(
                    "认证",
                    dll.trim(),
                    format!("[LSA {label}] {path}"),
                    "info",
                    Some(path),
                    None,
                );
            }
        }
    }
    // LSA\OSConfig\Security Packages（Win8+）
    if let Some(v) = reg_get_value(hkey_local(), &format!("{LSA_ROOT}\\OSConfig\\Security Packages"), "") {
        for dll in v.lines().filter(|s| !s.trim().is_empty()) {
            let path = format!(r"C:\Windows\System32\{}", dll.trim().to_ascii_lowercase());
            let path = if path.ends_with(".dll") { path } else { format!("{path}.dll") };
            sc.push("认证", dll.trim(), format!("[LSA 安全包(OSConfig)] {path}"), "info", Some(path), None);
        }
    }
}

/// SecurityProviders：SSP 提供程序 DLL 列表。
fn security_providers(sc: &mut Scanner) {
    if let Some(v) = reg_get_value(
        hkey_local(),
        &format!("{CC_ROOT}\\SecurityProviders"),
        "SecurityProviders",
    ) {
        for dll in v.split(&[',', ' '][..]).filter(|s| !s.trim().is_empty()) {
            let path = resolve_pe_path(dll.trim());
            sc.push(
                "认证",
                dll.trim(),
                format!("[SecurityProvider] {path}"),
                "info",
                Some(path),
                None,
            );
        }
    }
}

/// 凭据提供程序：登录界面加载的 COM DLL。
fn credential_providers(sc: &mut Scanner) {
    const ROOT: &str =
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Authentication\Credential Providers";
    for clsid in reg_subkeys(hkey_local(), ROOT) {
        let full = format!("{ROOT}\\{clsid}");
        let title = reg_get_value(hkey_local(), &full, "")
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string());
        let path = clsid_server(&clsid);
        let name = title.unwrap_or_else(|| clsid.clone());
        sc.push(
            "认证",
            &name,
            format!("[凭据提供程序] {clsid}"),
            "info",
            path,
            None,
        );
    }
}

/// 打印监视器：Spooler 服务加载的 DLL。
fn print_monitors(sc: &mut Scanner) {
    let root = format!("{CC_ROOT}\\Print\\Monitors");
    for name in reg_subkeys(hkey_local(), &root) {
        if let Some(driver) = reg_get_value(hkey_local(), &format!("{root}\\{name}"), "Driver")
            && !driver.trim().is_empty()
        {
            let path = if driver.contains('\\') {
                expand_env(driver.trim())
            } else {
                format!(r"C:\Windows\System32\{}", driver.trim().to_ascii_lowercase())
            };
            sc.push(
                "认证",
                &name,
                format!("[打印监视器] {path}"),
                "info",
                Some(path),
                None,
            );
        }
    }
}
