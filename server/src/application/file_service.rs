//! 应用层：文件传输编排（upload / download + 校验和 + 落库）。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::{Db, agent_repo::AgentRepo, file_transfer_repo::FileTransferRepo};
use helm_proto::pb::{FileChunk, FileRequest, ServerMessage, file_request, server_message};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use uuid::Uuid;

const CHUNK_SIZE: usize = 64 * 1024;

/// 文件传输用例。
#[derive(Clone)]
pub struct FileService {
    db: Db,
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
}

impl FileService {
    pub fn new(db: Db, registry: ConnectionRegistry, transfers: TransferRegistry) -> Self {
        Self {
            db,
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

        let host_id = AgentRepo::new(self.db.clone())
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;
        let ft = FileTransferRepo::new(self.db.clone())
            .create(host_id, "upload", remote_path, data.len() as i64)
            .await?;

        let req = ServerMessage {
            kind: Some(server_message::Kind::FileRequest(FileRequest {
                transfer_id: transfer_id.clone(),
                direction: file_request::Direction::Upload as i32,
                path: remote_path.to_string(),
                size: data.len() as u64,
                chunk_size: CHUNK_SIZE as u32,
            })),
        };
        self.registry
            .send(agent_id, req)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;

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
            self.registry
                .send(agent_id, msg)
                .await
                .map_err(|e| Error::NotConnected(e.to_string()))?;
            offset += piece.len() as u64;
        }

        let status = rx
            .await
            .map_err(|_| Error::Internal("transfer channel closed".into()))?;
        let ok = status.checksum == expected;
        FileTransferRepo::new(self.db.clone())
            .finish(ft.id, "done", data.len() as i64, &status.checksum)
            .await?;

        Ok((transfer_id, ok))
    }

    /// 从目标机取文件（download）。返回 (transfer_id, checksum 是否一致)。
    pub async fn download(
        &self,
        agent_id: &str,
        remote_path: &str,
        local_path: &str,
    ) -> Result<(String, bool)> {
        let transfer_id = Uuid::new_v4().to_string();

        let host_id = AgentRepo::new(self.db.clone())
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;
        let ft = FileTransferRepo::new(self.db.clone())
            .create(host_id, "download", remote_path, 0)
            .await?;

        let req = ServerMessage {
            kind: Some(server_message::Kind::FileRequest(FileRequest {
                transfer_id: transfer_id.clone(),
                direction: file_request::Direction::Download as i32,
                path: remote_path.to_string(),
                size: 0,
                chunk_size: CHUNK_SIZE as u32,
            })),
        };
        self.registry
            .send(agent_id, req)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;

        let (tx, rx) = oneshot::channel();
        self.transfers.register_download(&transfer_id, tx).await;

        let result = rx
            .await
            .map_err(|_| Error::Internal("transfer channel closed".into()))?;
        let local_checksum = hex::encode(Sha256::digest(&result.data));
        let ok = result.status.checksum == local_checksum;
        tokio::fs::write(local_path, &result.data).await?;

        FileTransferRepo::new(self.db.clone())
            .finish(
                ft.id,
                "done",
                result.data.len() as i64,
                &result.status.checksum,
            )
            .await?;

        Ok((transfer_id, ok))
    }
}
