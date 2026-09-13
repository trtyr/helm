//! 服务与驱动（Autoruns "Services / Drivers" 标签页）：
//! 注册表 Services 全量枚举（比 SCM 更快、可读 Description/svchost ServiceDll）。
//! 禁用态检测：Start=4 且存在 AutorunsDisabled 值（Autoruns 的服务禁用标记）。

use super::util::{
    Entry, Scanner, expand_env, extract_exe, reg_get_value, reg_subkeys, resolve_pe_path,
};
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";

pub fn scan(sc: &mut Scanner) {
    for name in reg_subkeys(HKEY_LOCAL_MACHINE, SERVICES) {
        let full = format!("{SERVICES}\\{name}");
        let get = |k: &str| reg_get_value(HKEY_LOCAL_MACHINE, &full, k);

        let image = get("ImagePath").unwrap_or_default();
        if image.trim().is_empty() {
            continue;
        }
        let type_bits = get("Type")
            .and_then(|t| t.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let start = get("Start")
            .and_then(|t| t.trim().parse::<u32>().ok())
            .unwrap_or(0);
        let display = get("DisplayName").filter(|s| !s.trim().is_empty());
        let desc = get("Description").filter(|s| !s.trim().is_empty());
        // Autoruns 禁用标记：原始 Start 存在 AutorunsDisabled 值里，Start 被改成 4
        let autoruns_disabled = get("AutorunsDisabled").is_some() && start == 4;

        // 内核驱动（SERVICE_KERNEL_DRIVER / FILE_SYSTEM_DRIVER）
        let is_driver = type_bits & 0x3 != 0;
        let start_label = match start {
            0 => "系统引导",
            1 => "系统启动",
            2 => "自动",
            3 => "手动",
            4 => "已禁用",
            _ => "其他",
        };

        // svchost 组服务：真实载荷在 Parameters\ServiceDll
        let mut exe = extract_exe(&image);
        if image.to_ascii_lowercase().contains("svchost.exe")
            && let Some(dll) = get(r"Parameters\ServiceDll")
        {
            let dll = resolve_pe_path(&expand_env(dll.trim().trim_matches('"')));
            if !dll.is_empty() {
                exe = Some(dll);
            }
        }

        let cat = if is_driver { "驱动" } else { "服务" };
        let mut e = Entry::new(
            cat,
            &name,
            format!("ImagePath: {image} | 启动: {start_label}"),
            "info",
            exe,
        )
        .op_key(format!("svc\u{1f}{name}"));
        e.desc = display.or(desc);
        if autoruns_disabled {
            e = e.disabled();
        }
        sc.push_entry(e);
    }
}
