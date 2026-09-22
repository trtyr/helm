//! 平台无关的文本与环境处理：CSV 行拆分、控制台编码解码、Windows 环境变量展开。
//!
//! **为什么单独成模块**（T4）：这些逻辑原本埋在 `ir/util/` 的 Windows-only 文件里，
//! 在 macOS 上**根本不参与编译** → 既不可测也不可复用。抽到本模块后：
//!
//! - **生产构建**：与原先完全等价（`ir::util` 对此再导出，调用点零改动）；
//! - **测试构建**：由 `#[cfg(any(windows, test))]` 纳入编译，
//!   **`cargo test -p helm-agent` 在 macOS 上即可覆盖它们**（本文件底部的单测）。
//!
//! 判断标准：函数只依赖字符串/字节与 `std`，不碰 Win32 API —— 这类逻辑没有理由被平台门控。

/// 单行 CSV 字段拆分（处理带引号、引号内逗号）。
///
/// 状态机：普通态（逗号分隔）/ 引号态（逗号是字面量）/ 双引号转义（`""` → `"`）。
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

/// 控制台输出解码（GBK → UTF-8）：Windows 中文控制台默认 GBK，直接当 UTF-8 会花屏。
pub fn decode_console(bytes: &[u8]) -> String {
    let (decoded, _, _) = encoding_rs::GBK.decode(bytes);
    decoded.into_owned()
}

/// 过滤路径里的 `..` 组件（体检新提项 T5：防路径穿越）。
///
/// 保留其余组件与分隔符风格（输入含 `\` 用 `\`，否则用 `/`）；前导分隔符（绝对路径）保留。
/// 用途：注册表里读到的可执行路径可能是攻击者可控的字符串，拼接前先去掉上跳段。
pub fn strip_parent_dir_segments(path: &str) -> String {
    let sep = if path.contains('\\') { '\\' } else { '/' };
    let parts: Vec<&str> = path.split(['\\', '/']).filter(|p| *p != "..").collect();
    parts.join(&sep.to_string())
}
/// 展开 %SystemRoot% 等常见环境变量（免 ExpandEnvironmentStringsW 的额外 FFI）。
///
/// 只做已知变量的字面替换（不查真实系统环境，`APPDATA` 例外——它随用户而异，必须实测）。
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

/// `USER_INFO_1.usri1_flags` 位掩码 → 中文标记（平台无关纯函数，便于在 macOS 上单测）。
///
/// 位定义见 `lmaccess.h`（**P006 P0-8 回归钉**）：
/// `0x0000_0002 UF_ACCOUNTDISABLE` · `0x0000_0020 UF_PASSWD_NOTREQD` ·
/// **`0x0001_0000 UF_DONT_EXPIRE_PASSWD`**。
///
/// 这里收位掩码而不是结构体，就是为了让这条判定能被非 Windows 平台的单测钉住：此前它误写成
/// `0x0100`（那是 `UF_TEMP_DUPLICATE_ACCOUNT`，临时域账户），结果「密码永不过期」**既漏报真命中、
/// 又把无关账户误报**——IR 结论直接判反。
pub fn user_flag_marks(flags: u32) -> Vec<&'static str> {
    const UF_ACCOUNTDISABLE: u32 = 0x0000_0002;
    const UF_PASSWD_NOTREQD: u32 = 0x0000_0020;
    const UF_DONT_EXPIRE_PASSWD: u32 = 0x0001_0000;

    let mut marks = Vec::new();
    if flags & UF_ACCOUNTDISABLE != 0 {
        marks.push("已禁用");
    }
    if flags & UF_DONT_EXPIRE_PASSWD != 0 {
        marks.push("密码永不过期");
    }
    if flags & UF_PASSWD_NOTREQD != 0 {
        marks.push("无需密码");
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_splits_plain_fields() {
        assert_eq!(split_csv_line("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn csv_keeps_comma_inside_quotes() {
        assert_eq!(
            split_csv_line(r#""a,b",c"#),
            vec!["a,b", "c"],
            "引号内的逗号是字面量"
        );
    }

    #[test]
    fn csv_unescapes_double_quotes() {
        assert_eq!(
            split_csv_line(r#""say ""hi""",x"#),
            vec![r#"say "hi""#, "x"]
        );
    }

    #[test]
    fn csv_single_field_and_empty() {
        assert_eq!(split_csv_line("only"), vec!["only"]);
        assert_eq!(split_csv_line(""), vec![""]);
    }

    #[test]
    fn csv_trailing_comma_yields_empty_tail() {
        assert_eq!(split_csv_line("a,"), vec!["a", ""]);
    }

    #[test]
    fn decode_console_decodes_gbk() {
        // "中文" 的 GBK 字节（D6 D0 CE C4）
        assert_eq!(decode_console(&[0xD6, 0xD0, 0xCE, 0xC4]), "中文");
    }

    #[test]
    fn decode_console_passes_ascii_through() {
        assert_eq!(decode_console(b"svchost.exe"), "svchost.exe");
    }

    #[test]
    fn expand_env_replaces_known_vars() {
        assert_eq!(
            expand_env(r"%SystemRoot%\System32\x.exe"),
            r"C:\Windows\System32\x.exe"
        );
        assert_eq!(expand_env("%windir%\\a"), r"C:\Windows\a");
        assert_eq!(expand_env("%SystemDrive%\\a"), r"C:\a");
    }

    #[test]
    fn expand_env_leaves_unknown_untouched() {
        assert_eq!(expand_env(r"C:\plain\path.exe"), r"C:\plain\path.exe");
        assert_eq!(expand_env(r"%NOT_A_VAR%\x"), r"%NOT_A_VAR%\x");
    }

    #[test]
    fn strip_parent_strips_windows_traversal() {
        assert_eq!(
            strip_parent_dir_segments(r"C:\Windows\..\..\evil.exe"),
            r"C:\Windows\evil.exe"
        );
        assert_eq!(strip_parent_dir_segments(r"..\evil.exe"), r"evil.exe");
    }

    #[test]
    fn strip_parent_strips_posix_traversal_and_keeps_root() {
        assert_eq!(strip_parent_dir_segments("/usr/../../etc/x"), "/usr/etc/x");
        assert_eq!(strip_parent_dir_segments("../../x"), "x");
    }

    #[test]
    fn strip_parent_leaves_clean_paths_alone() {
        assert_eq!(
            strip_parent_dir_segments(r"C:\Windows\System32\a.exe"),
            r"C:\Windows\System32\a.exe"
        );
    }

    #[test]
    fn expand_env_covers_known_spellings() {
        // 覆盖范围以表内拼写为准：%SystemRoot% / %systemroot% / %windir% / %WINDIR% …
        // （不做全大小写不敏感匹配——Windows 注册表里的常见拼写已覆盖；如需扩展改表即可）
        assert_eq!(
            expand_env("%SYSTEMROOT%\\a"),
            "%SYSTEMROOT%\\a",
            "全大写拼写不在表内（记录现状）"
        );
        assert_eq!(expand_env("%SystemRoot%\\a"), r"C:\Windows\a");
        assert_eq!(expand_env("%systemroot%\\a"), r"C:\Windows\a");
    }

    #[test]
    fn user_flag_marks_maps_known_bits() {
        assert!(user_flag_marks(0).is_empty());
        assert_eq!(user_flag_marks(0x0002), vec!["已禁用"]);
        assert_eq!(user_flag_marks(0x0020), vec!["无需密码"]);
        // P0-8 回归钉：0x0001_0000 才是「密码永不过期」
        assert_eq!(user_flag_marks(0x0001_0000), vec!["密码永不过期"]);
        assert_eq!(
            user_flag_marks(0x0002 | 0x0001_0000),
            vec!["已禁用", "密码永不过期"]
        );
    }

    #[test]
    fn user_flag_marks_ignores_temp_duplicate_bit() {
        // 0x0100 = UF_TEMP_DUPLICATE_ACCOUNT（不是「密码永不过期」）——误用会让 IR 判反
        assert!(user_flag_marks(0x0100).is_empty());
    }
}
