//! 通用查询注册表：桥接「下发请求 ↔ 回传结果」的一次性查询（进程/网络/系统服务）。

use helm_proto::pb::{
    AutorunsActionResult, FileMetaResult, IrScanResult, MemScanResult, NetInfoResult,
    ProcessKillResult, ProcessListResult, SysServiceActionResult, SysServiceListResult,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

/// 查询结果。
#[derive(Debug)]
pub enum QueryResponse {
    ProcessList(ProcessListResult),
    ProcessKill(ProcessKillResult),
    NetInfo(NetInfoResult),
    SysServiceList(SysServiceListResult),
    SysServiceAction(SysServiceActionResult),
    IrScan(IrScanResult),
    MemScan(MemScanResult),
    AutorunsAction(AutorunsActionResult),
    FileMeta(FileMetaResult),
    FsTimeline(helm_proto::pb::FsTimelineResult),
}

/// 待完成的查询请求：request_id → oneshot。
#[derive(Clone, Default)]
pub struct QueryRegistry {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<QueryResponse>>>>,
}

impl QueryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 pending 请求。
    pub async fn register(&self, request_id: String) -> oneshot::Receiver<QueryResponse> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().await.insert(request_id, tx);
        rx
    }

    /// 完成请求：把结果发给对应 receiver。
    pub async fn complete(&self, request_id: &str, resp: QueryResponse) {
        if let Some(tx) = self.inner.lock().await.remove(request_id) {
            let _ = tx.send(resp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use helm_proto::pb::{NetInfoResult, ProcessKillResult};

    #[tokio::test]
    async fn register_complete_roundtrip() {
        let reg = QueryRegistry::new();
        let rx = reg.register("r1".to_string()).await;
        reg.complete(
            "r1",
            QueryResponse::ProcessKill(ProcessKillResult {
                request_id: "r1".into(),
                pid: 42,
                ok: true,
                error: None,
            }),
        )
        .await;
        match rx.await.unwrap() {
            QueryResponse::ProcessKill(r) => {
                assert_eq!(r.pid, 42);
                assert!(r.ok);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[tokio::test]
    async fn complete_unknown_request_is_noop() {
        let reg = QueryRegistry::new();
        reg.complete(
            "unknown",
            QueryResponse::NetInfo(NetInfoResult {
                request_id: "unknown".into(),
                hostname: "h".into(),
                interfaces: vec![],
                connections: vec![],
            }),
        )
        .await;
    }
}
