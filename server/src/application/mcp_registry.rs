//! MCP 操作注册表：单工具 `helm` 的 op 目录（渐进式编目，见 docs/mcp.md）。
//!
//! 每个 op 是对平台既有 HTTP 端点的声明式映射（method + path 模板 + 参数说明），
//! 由 http/mcp.rs 翻译成 loopback 自调用——MCP 面 == HTTP 面，鉴权/审计零重复。
//! AI 看到的目录按其 API key 的 scope 裁剪、按 os 分组，实现渐进式发现。
//!
//! **文件布局（G7 拆分，2026-09-20）**——本文件只放类型、`op!` 宏与两域的合成器：
//!
//! | 文件 | 内容 |
//! |---|---|
//! | 本文件 | [`Os`] / [`OpDef`] / `op!` 宏 / [`ops`] 合成器 / 表不变量的单测 |
//! | [`ops_host`] | host 域 28 条数据（对主机做的一切） |
//! | [`ops_platform`] | platform 域 13 条数据（平台自身管理） |
//! | [`catalog`] | scope/os 裁剪、查找、占位符、工具描述分层、三级目录 JSON |
//!
//! 新增能力 = 在对应域文件加一条 + 在 `http::auth::scope_for` 编目（否则 fail-closed 拒绝）。
//!
//! ⚠ `scripts/gen_mcp_catalog.py` 按文本解析**两个域文件**（认 `// -- <域> 域` 注释与
//! `op!(` 块）生成 `console/src/lib/mcpCatalog.ts`；域文件结构变更需同步该脚本。

mod catalog;
mod ops_host;
mod ops_platform;

pub use catalog::{allowed_ops, catalog_json, find, placeholders, tool_description};
pub use ops_host::HOST_OPS;
pub use ops_platform::PLATFORM_OPS;

/// op 适用的操作系统维度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Any,
    Windows,
    Linux,
}

impl Os {
    pub fn as_str(self) -> &'static str {
        match self {
            Os::Any => "any",
            Os::Windows => "windows",
            Os::Linux => "linux",
        }
    }

    /// 解析调用方传入的 os 参数。
    pub fn parse(s: &str) -> Option<Os> {
        match s {
            "windows" => Some(Os::Windows),
            "linux" => Some(Os::Linux),
            _ => None,
        }
    }
}

/// 一个可被 AI 调用的操作。
pub struct OpDef {
    /// 操作名（点分域前缀，如 "hosts.list"）。
    pub name: &'static str,
    /// 所需 API key scope（与 application::scopes 对应）。
    pub scope: &'static str,
    pub os: Os,
    pub method: &'static str,
    /// HTTP 路径模板，`{x}` 占位符从 args 取值。
    pub path: &'static str,
    /// 中文一句话说明（工具索引与编目用）。
    pub summary: &'static str,
    /// 参数说明（args 的键 → 中文说明；path 占位符也在此声明）。
    pub params: &'static [(&'static str, &'static str)],
}

macro_rules! op {
    ($name:literal, $scope:literal, $os:expr, $method:literal, $path:literal, $summary:literal, $params:expr) => {
        OpDef {
            name: $name,
            scope: $scope,
            os: $os,
            method: $method,
            path: $path,
            summary: $summary,
            params: $params,
        }
    };
}
pub(crate) use op;

/// 全量操作注册表：host 域在前、platform 域在后（顺序即 `catalog` 的域顺序）。
///
/// 数据本身按域拆在 [`ops_host`] / [`ops_platform`]（各文件 < 400 行）；此函数是把两域
/// 合成一个序列的唯一入口，调用方无需关心分了多少文件。
pub fn ops() -> impl Iterator<Item = &'static OpDef> {
    HOST_OPS.iter().chain(PLATFORM_OPS.iter())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn op_names_are_unique_and_dot_prefixed() {
        let mut names: Vec<_> = ops().map(|o| o.name).collect();
        names.sort_unstable();
        let before = names.len();
        names.dedup();
        assert_eq!(before, names.len(), "op 名重复");
        assert!(
            ops().all(|o| o.name.split('.').next().unwrap_or("").len() > 1),
            "op 名必须带域前缀"
        );
    }

    #[test]
    fn every_op_scope_is_valid() {
        for op in ops() {
            assert!(
                crate::application::scopes::is_valid(op.scope),
                "op {} 引用了未定义 scope {}",
                op.name,
                op.scope
            );
        }
    }

    #[test]
    fn placeholder_extraction() {
        assert_eq!(placeholders("/api/v1/jobs/{id}"), vec!["id"]);
        assert_eq!(
            placeholders("/api/v1/services/{id}/{action}"),
            vec!["id", "action"]
        );
        assert!(placeholders("/api/v1/hosts").is_empty());
    }

    #[test]
    fn scope_filtering_hides_ir_without_scope() {
        let full = allowed_ops(&[], None);
        let restricted = allowed_ops(&["exec".to_string()], None);
        assert!(full.iter().any(|o| o.name == "ir.scan"));
        assert!(!restricted.iter().any(|o| o.name == "ir.scan"));
        assert!(restricted.iter().any(|o| o.name == "exec.run"));
        assert!(
            !tool_description(&["exec".to_string()], 3).contains("ir.scan"),
            "受限 key 的工具描述不应出现未授权 op"
        );
    }

    #[test]
    fn tool_description_respects_tier() {
        // tier 1：清单不展开，catalog 发现提示仍在
        let t1 = tool_description(&[], 1);
        assert!(!t1.contains("exec.run"), "tier 1 不应列 op 清单");
        assert!(t1.contains("catalog"), "tier 1 仍应提示 catalog 发现");

        // tier 2：host 域全量（ir 即暴露且带 windows 标注），platform 域不出现
        let t2 = tool_description(&[], 2);
        assert!(t2.contains("exec.run"));
        assert!(
            t2.contains("ir.scan"),
            "ir 应在 tier 2 即暴露（Windows 主机语义，owner 拍板）"
        );
        assert!(
            t2.contains("windows 专属"),
            "ir 应带 windows 专属标注（暴露但警示，Q2 倾向）"
        );
        assert!(
            !t2.contains("listeners.create"),
            "tier 2 不应含 platform 域 op"
        );

        // tier 3：全量（含 platform 管理能力）
        let t3 = tool_description(&[], 3);
        assert!(t3.contains("exec.run") && t3.contains("listeners.create"));
    }

    #[test]
    fn catalog_respects_os_filter() {
        // 当前注册表无 Linux 专属 op：Linux 过滤 = Any 全保留 + Windows 全剔除
        let catalog = catalog_json(&[], None, Some(Os::Linux), 3).to_string();
        assert!(
            !catalog.contains("\"os\":\"windows\""),
            "Linux 过滤后不应含 windows op"
        );
        assert!(catalog.contains("\"os\":\"any\""));
    }
}
