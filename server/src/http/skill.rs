//! Skill 包分发端点（决策 011）：下载 zip / 查看清单。
//!
//! 挂在受保护路由下，JWT / API key 均可——API key 是 skill 的取用凭据。

use crate::application::skill_package;
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

/// 下载完整包：GET /api/v1/skill
pub async fn download(State(_state): State<AppState>) -> Result<Response, Error> {
    let bytes = skill_package::build_zip()?;
    let disposition = format!("attachment; filename=\"{}\"", skill_package::zip_filename());
    let mut headers = header::HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        header::HeaderValue::from_static("application/zip"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        header::HeaderValue::from_str(&disposition)
            .map_err(|e| Error::Internal(format!("header: {e}")))?,
    );
    Ok((StatusCode::OK, headers, bytes).into_response())
}

/// 清单：GET /api/v1/skill/manifest（版本 + 文件 + sha256，用于升级对比）
pub async fn manifest(State(_state): State<AppState>) -> Result<Json<serde_json::Value>, Error> {
    Ok(Json(skill_package::manifest()))
}
