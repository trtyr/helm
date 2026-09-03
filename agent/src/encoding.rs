//! 控制台输出解码：Windows 中文/非 UTF-8 locale 下，子进程输出是 OEM 代码页
//! 编码（如简中 936/GBK），直接按 UTF-8 lossy 转换会乱码。此处按系统 OEM
//! 代码页解码为 UTF-8；非 Windows 平台一律按 UTF-8 处理。

/// 按指定代码页解码字节流（纯函数，便于测试）。
///
/// 仅映射 encoding_rs 支持的常见 OEM/ANSI 代码页；未识别的回退 UTF-8 lossy。
#[cfg_attr(not(windows), allow(dead_code))]
pub fn decode_with_codepage(codepage: u16, bytes: &[u8]) -> String {
    let encoding = match codepage {
        936 => encoding_rs::GBK,                       // 简体中文
        950 => encoding_rs::BIG5,                      // 繁体中文
        932 => encoding_rs::SHIFT_JIS,                 // 日语
        949 => encoding_rs::EUC_KR,                    // 韩语
        1252 | 850 | 437 => encoding_rs::WINDOWS_1252, // 西文
        1251 | 866 => encoding_rs::WINDOWS_1251,       // 西里尔
        1254 | 857 => encoding_rs::WINDOWS_1254,       // 土耳其
        1250 | 852 => encoding_rs::WINDOWS_1250,       // 中欧
        874 => encoding_rs::WINDOWS_874,               // 泰语
        1255 => encoding_rs::WINDOWS_1255,             // 希伯来
        1256 => encoding_rs::WINDOWS_1256,             // 阿拉伯
        1253 => encoding_rs::WINDOWS_1253,             // 希腊
        1257 => encoding_rs::WINDOWS_1257,             // 波罗的
        _ => return String::from_utf8_lossy(bytes).into_owned(),
    };
    let mut decoder = encoding.new_decoder();
    // UTF-8 最坏情况 3 字节/字符（encoding_rs 文档给的最坏上限）
    let mut out = String::with_capacity(bytes.len() * 3);
    let _ = decoder.decode_to_string(bytes, &mut out, true);
    out
}

#[cfg(windows)]
thread_local! {
    static OEM_CP: std::sync::OnceLock<u16> = const { std::sync::OnceLock::new() };
}

#[cfg(windows)]
fn oem_codepage() -> u16 {
    // GetOEMCP(): 系统控制台代码页（中文 Windows = 936）；返回 u32，codepage 范围内安全收窄
    OEM_CP.with(|cell| {
        *cell.get_or_init(|| unsafe { windows_sys::Win32::Globalization::GetOEMCP() as u16 })
    })
}

/// 解码控制台输出：Windows 按 OEM 代码页，其他平台按 UTF-8 lossy。
pub fn decode_console(bytes: &[u8]) -> String {
    #[cfg(windows)]
    {
        decode_with_codepage(oem_codepage(), bytes)
    }
    #[cfg(not(windows))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gbk_bytes_decode_correctly() {
        // "版本" 的 GBK 编码（Windows `ver` 输出首二字）
        let gbk: &[u8] = &[0xB0, 0xE6, 0xB1, 0xBE];
        assert_eq!(decode_with_codepage(936, gbk), "版本");
        // 真机观察到的乱码样例：GBK "中文" = [0xD6, 0xD0, 0xCE, 0xC4]
        assert_eq!(decode_with_codepage(936, &[0xD6, 0xD0, 0xCE, 0xC4]), "中文");
    }

    #[test]
    fn utf8_bytes_survive_gbk_decoder() {
        // 纯 ASCII 在任何代码页下不变
        assert_eq!(
            decode_with_codepage(936, b"DESKTOP-3M7DKO9"),
            "DESKTOP-3M7DKO9"
        );
    }

    #[test]
    fn unknown_codepage_falls_back_to_utf8_lossy() {
        // 合法 UTF-8 中文
        let utf8 = "中文".as_bytes();
        assert_eq!(decode_with_codepage(9999, utf8), "中文");
    }

    #[test]
    fn invalid_utf8_without_mapping_produces_replacement() {
        // 9999 回退 utf8 lossy：GBK 首字节非法 → 替换符
        assert!(decode_with_codepage(9999, &[0xB0, 0xE6]).contains('\u{FFFD}'));
    }
}
