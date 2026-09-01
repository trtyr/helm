//! 通用查询注册表：桥接「下发请求 ↔ 回传结果」的一次性查询（进程/网络）。

use helm_proto::pb::{NetInfoResult, ProcessKillResult, ProcessListResult};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

/// 查询结果。
#[derive(Debug)]
pub enum QueryResponse {
    ProcessList(ProcessListResult),
    ProcessKill(ProcessKillResult),
    NetInfo(NetInfoResult),
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
