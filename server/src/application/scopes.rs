//! API key scope 常量：一个 scope 对应一个功能域的路由集合（细分凭证，"什么功能什么凭证"）。
//!
//! - 空 `scopes` = 全功能（兼容存量 key，见 0016 迁移）。
//! - api-keys 管理与账号端点不设 scope，永远 JWT-only（见 http/auth.rs 的 fail-closed 规则）。
//! - 新增受保护路由时必须在 `http::auth::scope_for` 编目，否则 API key 一律 403。

pub const HOSTS: &str = "hosts";
pub const EXEC: &str = "exec";
pub const FILES: &str = "files";
pub const SERVICES: &str = "services";
pub const PROCESSES: &str = "processes";
pub const METRICS: &str = "metrics";
pub const NOTIFICATIONS: &str = "notifications";
pub const LISTENERS: &str = "listeners";
pub const FORWARD: &str = "forward";
pub const PROXY: &str = "proxy";
pub const IR: &str = "ir";
pub const AGENT_GEN: &str = "agent-gen";
pub const AUDIT: &str = "audit";

/// 全部合法 scope（创建 key 时校验的值域；JWT 视为持有全部）。
pub const ALL: &[&str] = &[
    HOSTS,
    EXEC,
    FILES,
    SERVICES,
    PROCESSES,
    METRICS,
    NOTIFICATIONS,
    LISTENERS,
    FORWARD,
    PROXY,
    IR,
    AGENT_GEN,
    AUDIT,
];

/// scope 是否合法。
pub fn is_valid(scope: &str) -> bool {
    ALL.contains(&scope)
}

/// 校验一批 scope，返回非法项（空输入合法 = 全功能）。
pub fn invalid_ones(scopes: &[String]) -> Vec<String> {
    scopes.iter().filter(|s| !is_valid(s)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_scopes_are_unique_and_valid() {
        let set: std::collections::HashSet<_> = ALL.iter().collect();
        assert_eq!(set.len(), ALL.len(), "scope 常量重复");
        assert!(ALL.iter().all(|s| is_valid(s)));
    }

    #[test]
    fn invalid_ones_reports_unknown() {
        let bad = invalid_ones(&["exec".into(), "nope".into()]);
        assert_eq!(bad, vec!["nope".to_string()]);
        assert!(invalid_ones(&[]).is_empty());
    }
}
