//! 列目录请求注册表：桥接 FileList（下发）↔ FileListResult（回传）。

use helm_proto::pb::FileListResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, oneshot};

/// 待完成的列目录请求：request_id → oneshot。
#[derive(Clone, Default)]
pub struct FileListRegistry {
    inner: Arc<Mutex<HashMap<String, oneshot::Sender<FileListResult>>>>,
}

impl FileListRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册一个 pending 请求，返回接收结果的 receiver。
    pub async fn register(&self, request_id: String) -> oneshot::Receiver<FileListResult> {
        let (tx, rx) = oneshot::channel();
        self.inner.lock().await.insert(request_id, tx);
        rx
    }

    /// 完成请求：把结果发给对应 receiver。
    pub async fn complete(&self, result: FileListResult) {
        if let Some(tx) = self.inner.lock().await.remove(&result.request_id) {
            let _ = tx.send(result);
        }
    }
}
