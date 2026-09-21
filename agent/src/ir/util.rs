//! IR 公共工具：扫描器上下文、严重级别启发式，以及对下层的**再导出**。
//!
//! **文件布局（G7 拆分，2026-09-20）**——本模块原为单文件 1093 行（全仓最大），现按原有分节拆为：
//!
//! | 文件 | 内容 |
//! |---|---|
//! | 本文件 | [`Scanner`]（发现汇总/去重/文件信息缓存）、[`Entry`]、`file_mtime`、严重级别启发式 |
//! | [`cmdline`] | 命令行 → 可执行文件路径（`expand_env` / `extract_exe` / `resolve_pe_path`） |
//! | [`signer`] | 厂商（版本资源）与 Authenticode/目录签名校验 |
//! | [`registry`] | 注册表读、写、Autoruns 禁用/恢复 |
//! | [`misc`] | 子进程命令、控制台解码、CSV 行拆分 |
//!
//! 所有子模块条目都在此 **`pub use` 再导出**，因此 `ir::util::X` 的调用路径与拆分前完全一致
//! （其余 ir 模块零改动）。

use helm_proto::pb::IrFinding;
use std::collections::HashMap;

mod cmdline;
mod misc;
mod registry;
mod signer;

pub use crate::ir::text::{decode_console, expand_env, split_csv_line};
pub use cmdline::{extract_exe, resolve_pe_path};
pub use misc::child_cmd;
pub use registry::{
    AUTORUNS_DISABLED_SUBKEY, clsid_server, hive_from_str, hkey_local, reg_delete_value,
    reg_get_raw, reg_get_value, reg_move_value_to_disabled, reg_restore_value_from_disabled,
    reg_set_raw, reg_subkeys, reg_values,
};
pub use signer::{file_publisher, file_sign_state, file_signer};

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
