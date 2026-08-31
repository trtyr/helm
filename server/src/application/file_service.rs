//! 应用层：文件传输编排（upload / download + 校验和）。

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use anyhow::Result;
use helm_proto::pb::{FileChunk, FileRequest, ServerMessage, file_request, server_message};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use uuid::Uuid;

const CHUNK_SIZE: usize = 64 * 1024;

/// 文件传输用例。
#[derive(Clone)]
pub struct FileService {
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
}

impl FileService {
    pub fn new(registry: ConnectionRegistry, transfers: TransferRegistry) -> Self {
        Self {
            registry,
            transfers,
        }
    }

    /// 下发文件到目标机（upload）。返回 (transfer_id, checksum 是否一致)。
    pub async fn upload(
        &self,
        agent_id: &str,
        local_path: &str,
        remote_path: &str,
    ) -> Result<(String, bool)> {
        let data = tokio::fs::read(local_path).await?;
        let expected = hex::encode(Sha256::digest(&data));
        let transfer_id = Uuid::new_v4().to_string();

        let req = ServerMessage {
            kind: Some(server_message::Kind::FileRequest(FileRequest {
                transfer_id: transfer_id.clone(),
                direction: file_request::Direction::Upload as i32,
                path: remote_path.to_string(),
                size: data.len() as u64,
                chunk_size: CHUNK_SIZE as u32,
            })),
        };
        self.registry.send(agent_id, req).await?;

        let (tx, rx) = oneshot::channel();
        self.transfers.register_upload(&transfer_id, tx).await;

        let mut offset = 0u64;
        for piece in data.chunks(CHUNK_SIZE) {
            let msg = ServerMessage {
                kind: Some(server_message::Kind::FileChunk(FileChunk {
                    transfer_id: transfer_id.clone(),
                    offset,
                    data: piece.to_vec(),
                })),
            };
            self.registry.send(agent_id, msg).await?;
            offset += piece.len() as u64;
        }

        let status = rx.await?;
        Ok((transfer_id, status.checksum == expected))
    }

    /// 从目标机取文件（download）。返回 (transfer_id, checksum 是否一致)。
    pub async fn download(
        &self,
        agent_id: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<(String, bool)> {
        let transfer_id = Uuid::new_v4().to_string();

        let req = ServerMessage {
            kind: Some(server_message::Kind::FileRequest(FileRequest {
                transfer_id: transfer_id.clone(),
                direction: file_request::Direction::Download as i32,
                path: remote_path.to_string(),
                size: 0,
                chunk_size: CHUNK_SIZE as u32,
            })),
        };
        self.registry.send(agent_id, req).await?;

        let (tx, rx) = oneshot::channel();
        self.transfers.register_download(&transfer_id, tx).await;

        let result = rx.await?;
        let local_checksum = hex::encode(Sha256::digest(&result.data));
        let ok = result.status.checksum == local_checksum;
        tokio::fs::write(local_path, &result.data).await?;

        Ok((transfer_id, ok))
    }
}
