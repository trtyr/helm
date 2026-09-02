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

        let mut metrics = vec![
            point("cpu.usage", cpu as f64, now),
            point("mem.used", mem_used as f64, now),
            point("mem.total", mem_total as f64, now),
            point("mem.percent", mem_percent, now),
            point("proc.count", proc_count as f64, now),
        ];

        // 磁盘使用率（每个挂载点一个指标）
        let disks = sysinfo::Disks::new_with_refreshed_list();
        for disk in disks.list() {
            let total = disk.total_space();
            let avail = disk.available_space();
            let usage = if total > 0 {
                (total - avail) as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let mount = disk.mount_point().to_string_lossy().to_string();
            metrics.push(point_labeled("disk.usage", usage, now, "mount", &mount));
        }

        // 网络累计收发（每个接口两个指标）
        let networks = sysinfo::Networks::new_with_refreshed_list();
        for (name, data) in networks.list() {
            metrics.push(point_labeled(
                "net.rx_bytes",
                data.received() as f64,
                now,
                "interface",
                name,
            ));
            metrics.push(point_labeled(
                "net.tx_bytes",
                data.transmitted() as f64,
                now,
                "interface",
                name,
            ));
        }

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

/// 带单个标签的指标点。
fn point_labeled(name: &str, value: f64, ts: u64, k: &str, v: &str) -> MetricPoint {
    let mut labels = HashMap::new();
    labels.insert(k.to_string(), v.to_string());
    MetricPoint {
        name: name.to_string(),
        value,
        labels,
        timestamp_unix_ms: ts,
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
