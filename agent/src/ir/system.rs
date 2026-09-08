//! 系统级持久化（Autoruns "Boot Execute / KnownDLLs / Winsock / Codecs" 标签页）：
//! 引导执行（Session Manager）、已知 DLL、Winsock LSP/命名空间/网络提供程序、编解码器（Drivers32）。

use super::util::{Entry, Scanner, expand_env, hkey_local, reg_get_value, reg_subkeys, reg_values, resolve_pe_path};

pub fn scan(sc: &mut Scanner) {
    boot_execute(sc);
    known_dlls(sc);
    winsock(sc);
    codecs(sc);
}

/// Boot Execute：smss.exe 在会话管理器阶段执行的原生镜像。
fn boot_execute(sc: &mut Scanner) {
    const SM: &str = r"SYSTEM\CurrentControlSet\Control\Session Manager";
    for (name, val) in reg_values(hkey_local(), SM) {
        if !matches!(
            name.as_str(),
            "BootExecute" | "SetupExecute" | "Execute" | "S0InitialCommand" | "S1InitialCommand"
        ) {
            continue;
        }
        // REG_MULTI_SZ：每行一个原生镜像名（System32 下，无扩展名）
        for item in val.lines().filter(|s| !s.trim().is_empty()) {
            let trimmed = item.trim();
            let base = trimmed.split(' ').next().unwrap_or(trimmed);
            let exe = resolve_pe_path(&format!("{base}.exe"));
            sc.push_entry(Entry::new(
                "引导执行",
                base,
                format!("[Session Manager\\{name}] {trimmed}"),
                "info",
                Some(exe),
            ));
        }
    }
}

/// KnownDLLs：引导期预加载并映射进所有进程的 DLL（劫持 = 全局注入）。
fn known_dlls(sc: &mut Scanner) {
    let root = r"SYSTEM\CurrentControlSet\Control\Session Manager\KnownDLLs";
    for (name, val) in reg_values(hkey_local(), root) {
        if name == "AlreadyTrained" || name == "TreatAs" || val.trim().is_empty() {
            continue;
        }
        let path = resolve_pe_path(&val);
        sc.push_entry(Entry::new(
            "已知 DLL",
            &name,
            format!("[KnownDLLs] {path}"),
            "info",
            Some(path),
        ));
    }
}

/// Winsock：协议目录（LSP）/ 命名空间提供程序 / 网络提供程序。
fn winsock(sc: &mut Scanner) {
    const WS2: &str = r"SYSTEM\CurrentControlSet\Services\WinSock2\Parameters";
    for cat in ["Protocol_Catalog9\\Catalog_Entries", "Protocol_Catalog9\\Catalog_Entries64", "NameSpace_Catalog5\\Catalog_Entries", "NameSpace_Catalog5\\Catalog_Entries64"] {
        let base = format!("{WS2}\\{cat}");
        for sub in reg_subkeys(hkey_local(), &base) {
            let full = format!("{base}\\{sub}");
            let get = |k: &str| reg_get_value(hkey_local(), &full, k).filter(|s| !s.trim().is_empty());
            let proto = get("ProtocolName");
            let lib = get("LibraryPath")
                .or_else(|| get("PackerLibraryName"))
                .or_else(|| get("LibraryName"));
            let Some(lib) = lib else { continue };
            let path = resolve_pe_path(&expand_env(lib.trim()));
            let title = proto.unwrap_or_else(|| sub.clone());
            sc.push_entry(Entry::new(
                "Winsock",
                &title,
                format!("[{cat}] {path}"),
                "info",
                Some(path),
            ));
        }
    }

    // 网络提供程序（Autoruns 归入 Winsock 标签）
    let root = r"SYSTEM\CurrentControlSet\Control\NetworkProvider";
    for name in reg_subkeys(hkey_local(), root) {
        if let Some(path_val) =
            reg_get_value(hkey_local(), &format!("{root}\\{name}"), "ProviderPath")
            && !path_val.trim().is_empty()
        {
            let path = expand_env(path_val.trim());
            sc.push_entry(Entry::new(
                "Winsock",
                &name,
                format!("[网络提供程序] {path}"),
                "info",
                Some(path),
            ));
        }
    }
}

/// 编解码器：Drivers32（视频 vidc.* / 音频 msacm.* / wave / midi 等）。
fn codecs(sc: &mut Scanner) {
    for root in [
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Drivers32",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows NT\CurrentVersion\Drivers32",
    ] {
        let tag = if root.contains("Wow6432Node") { "(32)" } else { "" };
        for (dev, dll) in reg_values(hkey_local(), root) {
            if dll.trim().is_empty() {
                continue;
            }
            let path = resolve_pe_path(&dll);
            sc.push_entry(Entry::new(
                "编解码器",
                &dev,
                format!("[Drivers32{tag}] {path}"),
                "info",
                Some(path),
            ));
        }
    }
}
