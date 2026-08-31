//! 状态监控：定期采集主机指标并上报。

use std::collections::HashMap;
use std::time::Duration;

use helm_proto::pb::{AgentMessage, MetricPoint, MetricReport, agent_message};
use tokio::sync::mpsc;

const MONITOR_INTERVAL: Duration = Duration::from_secs(30);

/// 监控循环：定期采集 CPU/内存/进程指标并发 MetricReport。
pub async fn run_monitor(tx: mpsc::Sender<AgentMessage>) {
    let mut sys = sysinfo::System::new_all();
    // 首次采样建立 CPU 使用率基准
    sys.refresh_cpu_usage();

    loop {
        tokio::time::sleep(MONITOR_INTERVAL).await;

        sys.refresh_memory();
        sys.refresh_cpu_usage();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        let cpu = sys.global_cpu_usage();
        let mem_total = sys.total_memory();
        let mem_used = sys.used_memory();
        let mem_percent = if mem_total > 0 {
            mem_used as f64 / mem_total as f64 * 100.0
        } else {
            0.0
        };
        let proc_count = sys.processes().len();
        let now = now_ms();

        let metrics = vec![
            point("cpu.usage", cpu as f64, now),
            point("mem.used", mem_used as f64, now),
            point("mem.total", mem_total as f64, now),
            point("mem.percent", mem_percent, now),
            point("proc.count", proc_count as f64, now),
        ];

        let msg = AgentMessage {
            kind: Some(agent_message::Kind::MetricReport(MetricReport { metrics })),
        };
        if tx.send(msg).await.is_err() {
            break;
        }
    }
}

fn point(name: &str, value: f64, ts: u64) -> MetricPoint {
    MetricPoint {
        name: name.to_string(),
        value,
        labels: HashMap::new(),
        timestamp_unix_ms: ts,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
