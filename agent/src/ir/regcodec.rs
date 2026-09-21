//! 注册表数据的**纯解码**逻辑：REG_SZ/EXPAND_SZ、DWORD、MULTI_SZ → 字符串。
//!
//! 抽自 `ir/util/registry.rs`（T4）：只吃「类型码 + 字节」，不碰任何 Win32 API ——
//! 这类逻辑没有理由被 Windows 门控，抽出来后 macOS 上即可 `cargo test` 覆盖。
//!
//! 生产路径不变：`ir::util::reg_*` 照旧调用本函数。

/// 注册表值 → 字符串。
///
/// - `REG_SZ`(1) / `REG_EXPAND_SZ`(2)：按 UTF-16LE 解码并去掉结尾 NUL；
/// - `REG_DWORD`(4)：按小端 u32 输出十进制；
/// - `REG_MULTI_SZ`(7)：按 NUL 分段、以换行连接；
/// - 其余：`binary(NB)`（避免把二进制当文本渲染成乱码）。
///
/// **长度上限（体检新提项 T5 复核结论）**：本函数不设上限，因为**上游读取已封顶**——
/// `reg_get_value` 用 4096 字节栈缓冲、`reg_get_raw` 用 64 KiB 堆缓冲，
/// 因此单值输出不可能超过 64 KiB。恶意超大注册表值在读取阶段即被截断，无需在此重复设限。
pub fn decode_reg_data(vtype: u32, data: &[u8]) -> String {
    match vtype {
        1 | 2 => {
            let u16s: Vec<u16> = data[..(data.len() / 2) * 2]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
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
                .as_chunks::<2>()
                .0
                .iter()
                .map(|c| u16::from_le_bytes(*c))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// UTF-16LE 编码助手（测试用）。
    fn u16le(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn reg_sz_decodes_utf16_and_trims_nul() {
        assert_eq!(decode_reg_data(1, &u16le("C:\\a.exe\0")), "C:\\a.exe");
    }

    #[test]
    fn reg_expand_sz_same_as_sz() {
        assert_eq!(
            decode_reg_data(2, &u16le("%SystemRoot%\\a\0")),
            "%SystemRoot%\\a"
        );
    }

    #[test]
    fn reg_dword_decodes_little_endian() {
        assert_eq!(decode_reg_data(4, &[0x2C, 0x01, 0x00, 0x00]), "300");
    }

    #[test]
    fn reg_dword_short_data_falls_back_to_binary() {
        // 长度不足 4 字节：不硬解，按二进制展示
        assert_eq!(decode_reg_data(4, &[1, 2]), "binary(2B)");
    }

    #[test]
    fn reg_multi_sz_joins_with_newline_and_drops_empty() {
        let mut data = u16le("a\0b\0");
        data.extend_from_slice(&u16le("\0")); // 结尾双 NUL
        assert_eq!(decode_reg_data(7, &data), "a\nb");
    }

    #[test]
    fn other_types_render_as_binary_len() {
        assert_eq!(decode_reg_data(3, &[9, 9, 9]), "binary(3B)");
        assert_eq!(decode_reg_data(0, &[]), "binary(0B)");
    }
}
