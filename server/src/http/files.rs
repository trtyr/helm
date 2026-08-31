//! 文件传输端点。

use crate::application::file_service::FileService;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let service = FileService::new(state.db, state.registry, state.transfers);
    match service
        .upload(&body.agent_id, &body.local_path, &body.remote_path)
        .await
    {
        Ok((transfer_id, checksum_ok)) => Ok(Json(json!({
            "transfer_id": transfer_id,
            "checksum_ok": checksum_ok,
        }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}

/// 取回文件：POST /api/v1/files/download
pub async fn download(
    State(state): State<AppState>,
    Json(body): Json<DownloadBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let service = FileService::new(state.db, state.registry, state.transfers);
    match service
        .download(&body.agent_id, &body.remote_path, &body.local_path)
        .await
    {
        Ok((transfer_id, checksum_ok)) => Ok(Json(json!({
            "transfer_id": transfer_id,
            "checksum_ok": checksum_ok,
        }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": e.to_string() })),
        )),
    }
}
