use std::path::PathBuf;
use std::time::Duration;

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

/// 日志保留清理（P003 T4）：删除 helm-agent.log.* 中修改时间超过 keep_days 的文件
///（每天一轮；失联期间 agent 本地挣扎记录可查但不无限增长——Q2 下限方案）。
/// keep_days <= 0 或 log_dir 为空时不清理。
pub fn spawn_log_retention(log_dir: &str, keep_days: i64) {
    if keep_days <= 0 || log_dir.is_empty() {
        return;
    }
    let dir = PathBuf::from(log_dir);
    tokio::spawn(async move {
        loop {
            let cutoff = std::time::SystemTime::now()
                - Duration::from_secs((keep_days.max(1) as u64) * 86_400);
            cleanup_once(&dir, cutoff);
            tokio::time::sleep(Duration::from_secs(86_400)).await;
        }
    });
}

fn cleanup_once(dir: &PathBuf, cutoff: std::time::SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !name.starts_with("helm-agent.log") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if let Ok(modified) = meta.modified()
            && modified < cutoff
        {
            let _ = std::fs::remove_file(entry.path()); // 轮转清理：删不掉（占用/权限）留待下轮，不阻断
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_once_removes_only_expired_agent_logs() {
        let dir = std::env::temp_dir().join(format!("helm-agent-log-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // 文件 mtime 是创建时刻（now），传「未来 cutoff」让所有文件视为过期，
        // 验证删除 + 前缀过滤逻辑（同 server telemetry 测试模式）
        std::fs::write(dir.join("helm-agent.log.2020-01-01"), b"old").unwrap();
        std::fs::write(dir.join("other.log.2020-01-01"), b"keep").unwrap();

        let cutoff = std::time::SystemTime::now() + Duration::from_secs(86_400);
        cleanup_once(&dir, cutoff);

        assert!(
            !dir.join("helm-agent.log.2020-01-01").exists(),
            "前缀匹配且早于 cutoff 的日志应被删除"
        );
        assert!(
            dir.join("other.log.2020-01-01").exists(),
            "非日志前缀文件不应被删"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
