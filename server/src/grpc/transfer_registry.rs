//! 文件传输等待表：upload 等 FileStatus，download 累积 chunk + 等 FileStatus。

use std::collections::HashMap;
use std::sync::Arc;

use helm_proto::pb::FileStatus;
use tokio::sync::{Mutex, oneshot};

/// download 的累积结果。
pub struct DownloadResult {
    pub data: Vec<u8>,
    pub status: FileStatus,
}

/// 等待中的传输。
pub enum PendingTransfer {
    Upload(oneshot::Sender<FileStatus>),
    Download {
        buf: Vec<u8>,
        tx: oneshot::Sender<DownloadResult>,
    },
}

/// 文件传输等待表。
#[derive(Clone, Default)]
pub struct TransferRegistry {
    pending: Arc<Mutex<HashMap<String, PendingTransfer>>>,
}

impl TransferRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_upload(&self, id: &str, tx: oneshot::Sender<FileStatus>) {
        self.pending
            .lock()
            .await
            .insert(id.to_string(), PendingTransfer::Upload(tx));
    }

    pub async fn register_download(&self, id: &str, tx: oneshot::Sender<DownloadResult>) {
        self.pending.lock().await.insert(
            id.to_string(),
            PendingTransfer::Download {
                buf: Vec::new(),
                tx,
            },
        );
    }

    /// 累积 download 的 chunk。
    pub async fn accumulate_chunk(&self, id: &str, data: &[u8]) {
        let mut map = self.pending.lock().await;
        if let Some(PendingTransfer::Download { buf, .. }) = map.get_mut(id) {
            buf.extend_from_slice(data);
        }
    }

    /// 收到 FileStatus，完成等待中的传输。
    pub async fn complete(&self, id: &str, status: FileStatus) {
        let entry = self.pending.lock().await.remove(id);
        match entry {
            Some(PendingTransfer::Upload(tx)) => {
                let _ = tx.send(status);
            }
            Some(PendingTransfer::Download { buf, tx }) => {
                let _ = tx.send(DownloadResult { data: buf, status });
            }
            None => {}
        }
    }
}
