//! 应用层：文件传输编排（upload / download + 校验和 + 落库）。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::{Db, agent_repo::AgentRepo, file_transfer_repo::FileTransferRepo};
use helm_proto::pb::{
    FileChunk, FileEntry, FileList, FileRequest, ServerMessage, file_request, server_message,
};
use sha2::{Digest, Sha256};
use tokio::sync::oneshot;
use uuid::Uuid;

const CHUNK_SIZE: usize = 64 * 1024;
const LIST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// 文件传输用例。
#[derive(Clone)]
pub struct FileService {
    db: Db,
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
    file_list: FileListRegistry,
}

impl FileService {
    pub fn new(
        db: Db,
        registry: ConnectionRegistry,
        transfers: TransferRegistry,
        file_list: FileListRegistry,
    ) -> Self {
        Self {
            db,
            registry,
            transfers,
            file_list,
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
        let expected = checksum(&data);
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
        let local_checksum = checksum(&result.data);
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

    /// 列目录：下发 FileList，等 FileListResult 回传。
    pub async fn list_dir(&self, agent_id: &str, path: &str) -> Result<Vec<FileEntry>> {
        let request_id = Uuid::new_v4().to_string();
        let rx = self.file_list.register(request_id.clone()).await;
        let msg = ServerMessage {
            kind: Some(server_message::Kind::FileList(FileList {
                request_id: request_id.clone(),
                path: path.to_string(),
            })),
        };
        self.registry
            .send(agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;

        let result = tokio::time::timeout(LIST_TIMEOUT, rx)
            .await
            .map_err(|_| Error::Internal("file list timeout".into()))?
            .map_err(|_| Error::Internal("file list channel closed".into()))?;

        if let Some(err) = result.error {
            return Err(Error::Internal(err));
        }
        Ok(result.entries)
    }
}

/// 计算 sha256 校验和（hex 编码，纯函数）。
pub fn checksum(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checksum_known_value() {
        // sha256("abc") 的标准值
        assert_eq!(
            checksum(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn checksum_differs_on_input() {
        assert_ne!(checksum(b"a"), checksum(b"b"));
    }
}
