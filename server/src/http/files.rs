//! 文件传输端点。

use crate::application::file_service::FileService;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use serde::Deserialize;
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

/// 下发文件：POST /api/v1/files/upload
pub async fn upload(
    State(state): State<AppState>,
    Json(body): Json<UploadBody>,
) -> Result<Json<Value>, Error> {
    let service = FileService::new(state.db, state.registry, state.transfers);
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
    Json(body): Json<DownloadBody>,
) -> Result<Json<Value>, Error> {
    let service = FileService::new(state.db, state.registry, state.transfers);
    let (transfer_id, checksum_ok) = service
        .download(&body.agent_id, &body.remote_path, &body.local_path)
        .await?;
    Ok(Json(json!({
        "transfer_id": transfer_id,
        "checksum_ok": checksum_ok,
    })))
}
