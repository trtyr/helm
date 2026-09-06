//! HTTP 错误边界：领域错误 → 安全响应。
//!
//! 单点错误边界：内部错误细节只进日志（tracing），对外只返回稳定 `code` + 安全消息，
//! 不泄漏 DB/文件 IO 等内部错误串。

use crate::domain::Error;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match self {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Error::Forbidden(_) => StatusCode::FORBIDDEN,
            Error::InvalidArgument(_) => StatusCode::BAD_REQUEST,
            Error::NotConnected(_) => StatusCode::CONFLICT,
            Error::Storage(_) | Error::Io(_) | Error::Internal(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };

        // 内部错误细节只进日志，不外泄
        tracing::error!(
            code = self.code(),
            retryable = self.retryable(),
            error = %self,
            "request failed"
        );

        let body = Json(json!({
            "error": {
                "code": self.code(),
                "message": self.safe_message(),
            }
        }));
        (status, body).into_response()
    }
}
