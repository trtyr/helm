use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer, fmt};

/// 初始化结构化日志。优先读 `RUST_LOG`，否则用配置默认级别。
/// `log_dir` 非空时追加按天滚动的文件日志（服务化/无控制台场景）。
pub fn init(default_level: &str, log_dir: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));

    let stdout_layer = fmt::layer().with_filter(filter.clone());

    if log_dir.is_empty() {
        tracing_subscriber::registry().with(stdout_layer).init();
        return;
    }

    let file_appender = tracing_appender::rolling::daily(log_dir, "helm-agent.log");
    let file_layer = fmt::layer().with_writer(file_appender).with_filter(filter);

    tracing_subscriber::registry()
        .with(stdout_layer)
        .with(file_layer)
        .init();
}
