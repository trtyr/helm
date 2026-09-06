//! 领域错误：类型化错误，带稳定 `code` 与 `retryable` 分类。

use thiserror::Error;

/// 领域/应用层统一错误类型。
#[derive(Debug, Error)]
pub enum Error {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("agent not connected: {0}")]
    NotConnected(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("io error: {0}")]
    Io(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl Error {
    /// 稳定错误码（对外可程序化识别）。
    pub fn code(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "not_found",
            Error::Unauthorized(_) => "unauthorized",
            Error::Forbidden(_) => "forbidden",
            Error::InvalidArgument(_) => "invalid_argument",
            Error::NotConnected(_) => "not_connected",
            Error::Storage(_) => "storage",
            Error::Io(_) => "io",
            Error::Internal(_) => "internal",
        }
    }

    /// 是否可重试（瞬时/基础设施类）。
    pub fn retryable(&self) -> bool {
        matches!(self, Error::NotConnected(_) | Error::Storage(_))
    }

    /// 对外安全消息：不泄漏内部错误细节。
    pub fn safe_message(&self) -> &'static str {
        match self {
            Error::NotFound(_) => "resource not found",
            Error::Unauthorized(_) => "unauthorized",
            Error::Forbidden(_) => "forbidden",
            Error::InvalidArgument(_) => "invalid request",
            Error::NotConnected(_) => "target agent not connected",
            Error::Storage(_) | Error::Io(_) | Error::Internal(_) => "internal server error",
        }
    }
}

impl From<sqlx::Error> for Error {
    fn from(e: sqlx::Error) -> Self {
        Error::Storage(e.to_string())
    }
}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

impl From<tonic::Status> for Error {
    fn from(e: tonic::Status) -> Self {
        Error::Internal(format!("grpc: {e}"))
    }
}

impl From<jsonwebtoken::errors::Error> for Error {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        Error::Unauthorized(format!("jwt: {e}"))
    }
}

pub type Result<T> = std::result::Result<T, Error>;
