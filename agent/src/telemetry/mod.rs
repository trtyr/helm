use tracing_subscriber::{EnvFilter, fmt};

/// 初始化结构化日志。优先读 `RUST_LOG`，否则用配置默认级别。
pub fn init(default_level: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    fmt().with_env_filter(filter).init();
}
