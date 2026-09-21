//! 文件传输端点。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::application::file_service::FileService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Extension, State};
use helm_proto::pb::FileEntry;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct UploadBody {
    pub agent_id: String,
    pub local_path: String,
    pub remote_path: String,
}

#[derive(Debug, Deserialize)]
pub struct DownloadBody {
    pub agent_id: String,
    pub remote_path: String,
    pub local_path: String,
}

#[derive(Debug, Deserialize)]
pub struct ListBody {
    pub agent_id: String,
    pub path: String,
}

/// 目录条目视图（proto 类型未启用 serde，手写可序列化视图）。
#[derive(Debug, Serialize)]
pub struct FileEntryView {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified_unix_ms: i64,
    pub mode: String,
}

impl From<FileEntry> for FileEntryView {
    fn from(e: FileEntry) -> Self {
        Self {
            name: e.name,
            is_dir: e.is_dir,
            size: e.size,
            modified_unix_ms: e.modified_unix_ms,
            mode: e.mode,
        }
    }
}

/// 下发文件：POST /api/v1/files/upload
pub async fn upload(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<UploadBody>,
) -> Result<Json<Value>, Error> {
    AuditService::new(state.db.clone())
        .record_best_effort(
            &claims.sub,
            "file_upload",
            &body.agent_id,
            json!({ "remote_path": body.remote_path }),
        )
        .await;
    let service = FileService::new(state.db, state.registry, state.transfers, state.file_list);
    let (transfer_id, checksum_ok) = service
        .upload(&body.agent_id, &body.local_path, &body.remote_path)
        .await?;
    Ok(Json(json!({
        "transfer_id": transfer_id,
        "checksum_ok": checksum_ok,
    })))
}

/// 取回文件：POST /api/v1/files/download
pub async fn download(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<DownloadBody>,
) -> Result<Json<Value>, Error> {
    AuditService::new(state.db.clone())
        .record_best_effort(
            &claims.sub,
            "file_download",
            &body.agent_id,
            json!({ "remote_path": body.remote_path }),
        )
        .await;
    let service = FileService::new(state.db, state.registry, state.transfers, state.file_list);
    let (transfer_id, checksum_ok) = service
        .download(&body.agent_id, &body.remote_path, &body.local_path)
        .await?;
    Ok(Json(json!({
        "transfer_id": transfer_id,
        "checksum_ok": checksum_ok,
    })))
}

/// 列目录：POST /api/v1/files/list
pub async fn list(
    State(state): State<AppState>,
    Json(body): Json<ListBody>,
) -> Result<Json<Value>, Error> {
    let service = FileService::new(state.db, state.registry, state.transfers, state.file_list);
    let entries: Vec<FileEntryView> = service
        .list_dir(&body.agent_id, &body.path)
        .await?
        .into_iter()
        .map(FileEntryView::from)
        .collect();
    Ok(Json(json!({
        "path": body.path,
        "entries": entries,
    })))
}
