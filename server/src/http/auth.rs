//! 认证：登录端点 + JWT 校验中间件。

use crate::application::audit_service::AuditService;
use crate::application::auth_service::{AuthService, Claims};
use crate::domain::Error;
use crate::http::AppState;
use axum::Json;
use axum::extract::{Extension, Request, State};
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

/// 改密参数。
#[derive(Debug, Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: String,
    pub new_password: String,
}

/// 改用户名参数。
#[derive(Debug, Deserialize)]
pub struct ChangeUsernameBody {
    pub current_password: String,
    pub new_username: String,
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

/// 账号管理端点仅限控制台用户（JWT）；API key 无"账号"概念。
fn require_jwt_role(claims: &Claims) -> Result<(), Error> {
    if claims.role == crate::application::api_key_service::ROLE_API_KEY {
        return Err(Error::Forbidden(
            "api keys have no account; login with JWT".into(),
        ));
    }
    Ok(())
}

/// 当前账号：GET /api/v1/auth/me
pub async fn me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Value>, Error> {
    require_jwt_role(&claims)?;
    let account = AuthService::new(state.db, state.jwt_secret)
        .account(&claims.sub)
        .await?;
    Ok(Json(json!({ "account": account })))
}

/// 修改密码：POST /api/v1/auth/change-password
pub async fn change_password(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<ChangePasswordBody>,
) -> Result<Json<Value>, Error> {
    require_jwt_role(&claims)?;
    AuthService::new(state.db.clone(), state.jwt_secret)
        .change_password(&claims.sub, &body.current_password, &body.new_password)
        .await?;
    let _ = AuditService::new(state.db)
        .record(&claims.sub, "password_change", "", json!({}))
        .await;
    Ok(Json(json!({ "ok": true })))
}

/// 修改用户名：POST /api/v1/auth/change-username（旧 token 随即失效，前端强制重登）
pub async fn change_username(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<ChangeUsernameBody>,
) -> Result<Json<Value>, Error> {
    require_jwt_role(&claims)?;
    let new_username = body.new_username.trim().to_string();
    AuthService::new(state.db.clone(), state.jwt_secret)
        .change_username(&claims.sub, &body.current_password, &new_username)
        .await?;
    let _ = AuditService::new(state.db)
        .record(&claims.sub, "username_change", &new_username, json!({}))
        .await;
    Ok(Json(json!({ "ok": true, "username": new_username })))
}

/// JWT / API key 校验中间件：校验 Bearer token，claims 塞进 request extension。
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

    let claims = verify_bearer_token(&state, token)
        .await
        .map_err(|e| e.into_response())?;

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

/// 校验 Bearer token（JWT 或 `helm_` API key），返回统一 Claims。
///
/// HTTP 中间件与 WS 端点（query param token，见 stream.rs / terminal.rs）共用：
/// `helm_` 前缀走 API key 哈希比对，其余按 JWT 解码。
pub async fn verify_bearer_token(state: &AppState, token: &str) -> Result<Claims, Error> {
    if token.starts_with(crate::application::api_key_service::RAW_PREFIX) {
        let svc = crate::application::api_key_service::ApiKeyService::new(state.db.clone());
        let name = svc
            .verify(token)
            .await?
            .ok_or_else(|| Error::Unauthorized("invalid token".into()))?;
        Ok(crate::application::api_key_service::claims_for(&name))
    } else {
        let auth = AuthService::new(state.db.clone(), state.jwt_secret.clone());
        auth.verify(token)
    }
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": msg }))).into_response()
}
