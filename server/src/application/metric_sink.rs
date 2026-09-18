//! 指标落库队列（E2）：MetricReport 的入库/告警/广播移出 gRPC 入站路径。
//!
//! 入站处理对 MetricReport 只做一次有界 enqueue（通道满即丢弃并计数），
//! ExecResult / FileStatus 等结果类消息不再被逐条 `INSERT` 拖住；真正的落库、
//! 阈值告警评估、通知与实时流广播由 [`spawn`] 启动的独立任务完成。

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use helm_proto::pb::MetricPoint;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::grpc::stream_registry::StreamRegistry;
use crate::store::Db;

/// 有界队列容量：满则整批丢弃（指标可再生产，优先保执行类消息畅通）。
pub const METRIC_QUEUE_CAPACITY: usize = 4096;

/// 单次落库循环合并的最大批大小（防 backlog 瞬时放大）。
const MAX_BATCH: usize = 256;

struct SinkMsg {
    host_id: Uuid,
    metrics: Vec<MetricPoint>,
}

/// 指标入队句柄。`Clone` 共享底层队列与计数器。
#[derive(Clone)]
pub struct MetricSink {
    tx: mpsc::Sender<SinkMsg>,
    /// 已入队未处理完的批数（测试等待落库收敛用）。
    in_flight: Arc<AtomicU64>,
    /// 因队列满被丢弃的报告总数（可观测：warn 日志 + 计数）。
    dropped: Arc<AtomicU64>,
}

impl MetricSink {
    /// 启动独立落库任务并返回入队句柄。
    pub fn spawn(db: Db, streams: StreamRegistry) -> Self {
        Self::spawn_with_capacity(db, streams, METRIC_QUEUE_CAPACITY)
    }

    /// 指定容量启动（测试用小值验证丢弃路径）。
    pub fn spawn_with_capacity(db: Db, streams: StreamRegistry, capacity: usize) -> Self {
        let (tx, rx) = mpsc::channel(capacity.max(1));
        let in_flight = Arc::new(AtomicU64::new(0));
        let dropped = Arc::new(AtomicU64::new(0));
        tokio::spawn(run_sink(
            rx,
            db,
            streams,
            in_flight.clone(),
            dropped.clone(),
        ));
        Self {
            tx,
            in_flight,
            dropped,
        }
    }

    /// 入队一批指标（非阻塞）：队列满时丢弃整批并递增丢弃计数（E2 可观测）。
    pub async fn enqueue(&self, host_id: Uuid, metrics: Vec<MetricPoint>) {
        if metrics.is_empty() {
            return;
        }
        self.in_flight.fetch_add(1, Ordering::Relaxed);
        if self.tx.try_send(SinkMsg { host_id, metrics }).is_err() {
            self.in_flight.fetch_sub(1, Ordering::Relaxed);
            let total = self.dropped.fetch_add(1, Ordering::Relaxed) + 1;
            tracing::warn!(
                dropped_total = total,
                queue_capacity = self.tx.capacity(),
                "metric queue full, report dropped"
            );
        }
    }

    /// 队列满丢弃总数（运维可观测入口）。
    pub fn dropped_total(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// 等待已入队消息全部处理完（测试辅助）：超时返回 false。
    pub async fn wait_idle(&self, timeout: std::time::Duration) -> bool {
        let deadline = tokio::time::Instant::now() + timeout;
        while self.in_flight.load(Ordering::Relaxed) > 0 {
            if tokio::time::Instant::now() >= deadline {
                return false;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        true
    }
}

/// 独立落库任务：批量取消息，逐条落库 + 阈值告警 + 通知 + 实时流广播。
async fn run_sink(
    mut rx: mpsc::Receiver<SinkMsg>,
    db: Db,
    streams: StreamRegistry,
    in_flight: Arc<AtomicU64>,
    _dropped: Arc<AtomicU64>,
) {
    let metric_repo = crate::store::metric_repo::MetricRepo::new(db.clone());
    let alert_repo = crate::store::alert_repo::AlertRepo::new(db.clone());
    while let Some(first) = rx.recv().await {
        let mut batch = vec![first];
        while batch.len() < MAX_BATCH {
            match rx.try_recv() {
                Ok(msg) => batch.push(msg),
                Err(_) => break,
            }
        }
        for msg in batch {
            for m in &msg.metrics {
                let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(
                    m.timestamp_unix_ms as i64,
                )
                .unwrap_or_else(chrono::Utc::now);
                if let Err(e) = metric_repo.insert(msg.host_id, &m.name, m.value, ts).await {
                    tracing::warn!(error = %e, "failed to persist metric");
                }
                // 告警评估：超阈值落 alerts 表
                if let Some(threshold) =
                    crate::application::alert_service::AlertService::threshold_for(&m.name)
                        .filter(|t| m.value > *t)
                {
                    let _ = alert_repo
                        .insert(msg.host_id, &m.name, threshold, m.value)
                        .await;
                    // 预警联动通知中心（决策 009：系统内小卡片）
                    let svc = crate::application::notification_service::NotificationService::new(
                        db.clone(),
                        streams.clone(),
                    );
                    let _ = svc
                        .notify(
                            msg.host_id,
                            crate::application::notification_service::KIND_ALERT,
                            &format!("预警：{} = {:.1}（阈值 {}）", m.name, m.value, threshold),
                        )
                        .await;
                }
                // 实时流：推送指标
                let payload = serde_json::json!({
                    "host_id": msg.host_id,
                    "name": m.name,
                    "value": m.value,
                    "ts": m.timestamp_unix_ms,
                });
                streams
                    .broadcast("metrics", payload.to_string().into_bytes())
                    .await;
            }
            // 整条报告处理完才计完成（wait_idle 的收敛语义）
            in_flight.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn queue_full_drops_and_counts() {
        // 容量 1：塞两条、消费端不取 → 第二条丢弃 + 计数
        let db = crate::store::Db::connect(
            &std::env::var("HELM_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm_itest".into()),
        )
        .await
        .expect("db");
        let sink = MetricSink::spawn_with_capacity(db, StreamRegistry::new(), 1);
        let points = vec![MetricPoint {
            name: "cpu.usage".into(),
            value: 1.0,
            labels: Default::default(),
            timestamp_unix_ms: 0,
        }];
        sink.enqueue(Uuid::new_v4(), points.clone()).await;
        sink.enqueue(Uuid::new_v4(), points).await;
        assert_eq!(
            sink.dropped_total(),
            1,
            "second report must be dropped (E2)"
        );
        assert!(sink.wait_idle(std::time::Duration::from_secs(2)).await);
    }

    #[tokio::test]
    async fn empty_report_is_ignored() {
        let db = crate::store::Db::connect(
            &std::env::var("HELM_DATABASE_URL")
                .unwrap_or_else(|_| "postgres://helm:helm@localhost:5433/helm_itest".into()),
        )
        .await
        .expect("db");
        let sink = MetricSink::spawn_with_capacity(db, StreamRegistry::new(), 4);
        sink.enqueue(Uuid::new_v4(), vec![]).await;
        assert_eq!(sink.dropped_total(), 0);
    }
}
