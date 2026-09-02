//! 认证：登录端点 + JWT 校验中间件。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::{AuthService, Claims};
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub username: String,
    pub password: String,
}

/// 登录：POST /api/v1/auth/login
pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<Value>, Error> {
    let auth = AuthService::new(state.db.clone(), state.jwt_secret.clone());
    match auth.login(&body.username, &body.password).await? {
        Some(token) => {
            let _ = AuditService::new(state.db)
                .record(&body.username, "login", "", json!({}))
                .await;
            Ok(Json(json!({ "token": token })))
        }
        None => Err(Error::Unauthorized("invalid credentials".into())),
    }
}

/// JWT 校验中间件：校验 Bearer token，claims 塞进 request extension。
// axum 中间件标准签名为 `Result<Response, Response>`，此处 Err 大是框架约定。
#[allow(clippy::result_large_err)]
pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, Response> {
    let token = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| unauthorized("missing token"))?;

    let auth = AuthService::new(state.db, state.jwt_secret);
    let claims: Claims = auth
        .verify(token)
        .map_err(|_| unauthorized("invalid token"))?;

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": msg }))).into_response()
}
