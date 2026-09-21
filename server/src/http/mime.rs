//! 静态资源 MIME 表（Vite 产物常用类型）。
//!
//! 抽自 `http/mod.rs`（T4）：纯查表逻辑，与 axum/路由无关 → 单独立文并补单测
//! （原先它是 `mod.rs` 里的私有函数，混在路由与静态托管之间，测试无从下手）。
//!
//! 语义保持：只按扩展名匹配；覆盖不全时返回 `application/octet-stream`（浏览器按猜测
//! 处理也无碍）——与拆分前逐字一致。

/// 按路径扩展名返回 MIME。大小写不敏感（Vite 产物偶有 `.PNG` 之类）。
pub(super) fn mime_of(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_vite_assets() {
        assert_eq!(
            mime_of("assets/index-abc123.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            mime_of("assets/index-abc123.css"),
            "text/css; charset=utf-8"
        );
        assert_eq!(mime_of("index.html"), "text/html; charset=utf-8");
        assert_eq!(mime_of("assets/logo.svg"), "image/svg+xml");
    }

    #[test]
    fn maps_fonts_and_images() {
        assert_eq!(mime_of("fonts/inter.woff2"), "font/woff2");
        assert_eq!(mime_of("favicon.ico"), "image/x-icon");
        assert_eq!(mime_of("img/a.webp"), "image/webp");
    }

    #[test]
    fn unknown_and_extensionless_fall_back() {
        assert_eq!(mime_of("notes.md"), "application/octet-stream");
        assert_eq!(mime_of("no-extension"), "application/octet-stream");
        assert_eq!(mime_of(""), "application/octet-stream");
    }

    #[test]
    fn extension_match_is_case_insensitive() {
        assert_eq!(mime_of("IMAGE.PNG"), "image/png");
        assert_eq!(mime_of("INDEX.HTML"), "text/html; charset=utf-8");
    }
}
