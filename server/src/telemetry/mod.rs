use std::path::PathBuf;
use std::time::Duration;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// 初始化结构化日志。优先读 `RUST_LOG`，否则用配置默认级别。
///
/// `log_dir` 为 `Some` 时双写 stdout + 按天轮转文件（helm-server.log.YYYY-MM-DD，
/// P003 T3：终结 /tmp 裸奔），返回非阻塞 writer 的 WorkerGuard——调用方须持有到
/// 进程退出，否则缓冲中的日志丢失。`None` 时仅 stdout（旧行为）。
pub fn init(default_level: &str, log_dir: Option<&PathBuf>) -> Option<WorkerGuard> {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_level));
    match log_dir {
        Some(dir) => {
            let appender = tracing_appender::rolling::daily(dir, "helm-server.log");
            let (writer, guard) = tracing_appender::non_blocking(appender);
            tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer())
                .with(fmt::layer().with_writer(writer))
                .init();
            Some(guard)
        }
        None => {
            fmt().with_env_filter(filter).init();
            None
        }
    }
}

/// 日志保留清理（P003 T3）：删除 helm-server.log.* 中修改时间早于 cutoff 的文件
///（每天一轮；cutoff 由 keep_days 复用 config.retention_days 口径计算）。
pub fn spawn_log_retention(log_dir: PathBuf, keep_days: i64) {
    if keep_days <= 0 {
        return;
    }
    tokio::spawn(async move {
        loop {
            let cutoff = std::time::SystemTime::now()
                - Duration::from_secs((keep_days.max(1) as u64) * 86_400);
            cleanup_once(&log_dir, cutoff);
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
        if !name.starts_with("helm-server.log") {
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
            let _ = std::fs::remove_file(entry.path()); // 轮转清理：删不掉（占用/权限）留待下轮，不阻断（下一行已留 info 日志）
            tracing::info!(file = %name, "log retention: removed old log file");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleanup_once_removes_only_expired_log_files() {
        let dir = std::env::temp_dir().join(format!("helm-log-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // 文件 mtime 是「创建时刻」（now），无法直接构造过期文件——改为传「未来 cutoff」：
        // 所有文件的 mtime（now）< cutoff 都视为过期，验证删除 + 前缀过滤逻辑
        std::fs::write(dir.join("helm-server.log.2020-01-01"), b"old").unwrap();
        std::fs::write(dir.join("other.log.2020-01-01"), b"keep").unwrap();

        let cutoff = std::time::SystemTime::now() + Duration::from_secs(86_400);
        cleanup_once(&dir, cutoff);

        assert!(
            !dir.join("helm-server.log.2020-01-01").exists(),
            "前缀匹配且早于 cutoff 的日志应被删除"
        );
        assert!(
            dir.join("other.log.2020-01-01").exists(),
            "非日志前缀文件不应被删"
        );

        // 清理测试目录
        let _ = std::fs::remove_dir_all(&dir);
    }
}
