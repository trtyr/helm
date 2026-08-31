//! 文件传输：处理 FileRequest / FileChunk，分块传输 + sha256 校验和。

use std::collections::HashMap;

use helm_proto::pb::{
    AgentMessage, FileChunk, FileRequest, FileStatus, agent_message, file_request::Direction,
    file_status::State as FileState,
};
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;

/// upload 会话：累积 chunk 直到达到预期大小。
struct UploadSession {
    path: String,
    size: u64,
    buf: Vec<u8>,
}

/// 文件传输处理：维护 upload 会话，处理 download。
pub struct FileHandler {
    uploads: HashMap<String, UploadSession>,
}

impl FileHandler {
    pub fn new() -> Self {
        Self {
            uploads: HashMap::new(),
        }
    }

    /// 处理 FileRequest。
    pub async fn handle_request(&mut self, req: FileRequest, tx: &mpsc::Sender<AgentMessage>) {
        match req.direction {
            d if d == Direction::Upload as i32 => {
                self.uploads.insert(
                    req.transfer_id.clone(),
                    UploadSession {
                        path: req.path.clone(),
                        size: req.size,
                        buf: Vec::with_capacity(req.size as usize),
                    },
                );
            }
            d if d == Direction::Download as i32 => {
                self.do_download(&req, tx).await;
            }
            _ => {}
        }
    }

    /// 处理 FileChunk（upload 方向），累积完成后写盘并回报。
    pub async fn handle_chunk(&mut self, chunk: FileChunk, tx: &mpsc::Sender<AgentMessage>) {
        let tid = chunk.transfer_id.clone();
        let complete = {
            let session = match self.uploads.get_mut(&tid) {
                Some(s) => s,
                None => return,
            };
            session.buf.extend_from_slice(&chunk.data);
            session.buf.len() as u64 >= session.size
        };
        if complete && let Some(session) = self.uploads.remove(&tid) {
            self.finish_upload(&tid, session, tx).await;
        }
    }

    async fn finish_upload(
        &self,
        tid: &str,
        session: UploadSession,
        tx: &mpsc::Sender<AgentMessage>,
    ) {
        let checksum = match std::fs::write(&session.path, &session.buf) {
            Ok(_) => hex::encode(Sha256::digest(&session.buf)),
            Err(e) => {
                send_status(tx, tid, FileState::Failed, 0, e.to_string(), String::new()).await;
                return;
            }
        };
        send_status(
            tx,
            tid,
            FileState::Done,
            session.buf.len() as u64,
            String::new(),
            checksum,
        )
        .await;
    }

    async fn do_download(&self, req: &FileRequest, tx: &mpsc::Sender<AgentMessage>) {
        let data = match std::fs::read(&req.path) {
            Ok(d) => d,
            Err(e) => {
                send_status(
                    tx,
                    &req.transfer_id,
                    FileState::Failed,
                    0,
                    e.to_string(),
                    String::new(),
                )
                .await;
                return;
            }
        };
        let checksum = hex::encode(Sha256::digest(&data));
        let chunk_size = req.chunk_size.max(1) as usize;
        let mut offset = 0u64;
        for piece in data.chunks(chunk_size) {
            let msg = AgentMessage {
                kind: Some(agent_message::Kind::FileChunk(FileChunk {
                    transfer_id: req.transfer_id.clone(),
                    offset,
                    data: piece.to_vec(),
                })),
            };
            if tx.send(msg).await.is_err() {
                return;
            }
            offset += piece.len() as u64;
        }
        send_status(
            tx,
            &req.transfer_id,
            FileState::Done,
            data.len() as u64,
            String::new(),
            checksum,
        )
        .await;
    }
}

async fn send_status(
    tx: &mpsc::Sender<AgentMessage>,
    tid: &str,
    state: FileState,
    bytes: u64,
    err: String,
    checksum: String,
) {
    let msg = AgentMessage {
        kind: Some(agent_message::Kind::FileStatus(FileStatus {
            transfer_id: tid.to_string(),
            state: state as i32,
            bytes_transferred: bytes,
            error: err,
            checksum,
        })),
    };
    let _ = tx.send(msg).await;
}
