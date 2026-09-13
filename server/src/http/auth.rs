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
/// API key 另做路由级 scope 强制（fail-closed：未编目路径一律拒绝，见 [`scope_for`]）。
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

    // API key 的细粒度授权：按 (方法, 路由模式) 查所需 scope；未编目路径拒绝。
    // JWT（人类控制台）不受 scope 约束。
    if claims.role == crate::application::api_key_service::ROLE_API_KEY {
        let matched = request
            .extensions()
            .get::<axum::extract::MatchedPath>()
            .map(|p| p.as_str().to_string())
            .unwrap_or_else(|| request.uri().path().to_string());
        let required = scope_for(&matched);
        let allowed = match required {
            Some(scope) => claims.has_scope(scope),
            None => false,
        };
        if !allowed {
            let _ = AuditService::new(state.db.clone())
                .record(
                    &claims.sub,
                    "api_key_denied",
                    &matched,
                    json!({ "scope": required, "method": request.method().as_str() }),
                )
                .await;
            tracing::warn!(actor = %claims.sub, path = %matched, scope = ?required, "api key scope denied");
            return Err(forbidden(
                required,
                "api key is missing the required scope for this route",
            ));
        }
    }

    request.extensions_mut().insert(claims);
    Ok(next.run(request).await)
}

/// 路由模式 → 所需 scope 的编目表（单一事实源，与 http/mod.rs 的路由注册一一对应）。
///
/// 返回 `None` 的含义分两类：
/// - api-keys / auth 账号路径：故意不编目——API key 一律 403（与 handler 内 require_jwt 双保险）；
/// - 其他未列出路径：fail-closed，新路由必须显式归属 scope，否则 API key 不可用。
pub fn scope_for(path: &str) -> Option<&'static str> {
    use crate::application::scopes as s;
    match path {
        // hosts 域（主机 + agent 档案）
        "/api/v1/hosts"
        | "/api/v1/hosts/{id}"
        | "/api/v1/hosts/{id}/tags"
        | "/api/v1/agents"
        | "/api/v1/agents/{id}"
        | "/api/v1/agents/{id}/tags"
        | "/api/v1/agents/{id}/uninstall" => Some(s::HOSTS),
        // exec 域
        "/api/v1/exec"
        | "/api/v1/exec/batch"
        | "/api/v1/jobs"
        | "/api/v1/jobs/{id}"
        | "/api/v1/tasks/script"
        | "/api/v1/tasks/schedule" => Some(s::EXEC),
        "/api/v1/files/upload" | "/api/v1/files/download" | "/api/v1/files/list" => Some(s::FILES),
        // services 域（常驻服务 + 系统服务）
        "/api/v1/services"
        | "/api/v1/services/{id}"
        | "/api/v1/services/{id}/start"
        | "/api/v1/services/{id}/stop"
        | "/api/v1/services/{id}/restart"
        | "/api/v1/services/{id}/logs"
        | "/api/v1/sys-services/list"
        | "/api/v1/sys-services/action" => Some(s::SERVICES),
        "/api/v1/processes/list" | "/api/v1/processes/kill" | "/api/v1/net/info" => {
            Some(s::PROCESSES)
        }
        "/api/v1/metrics" => Some(s::METRICS),
        "/api/v1/alerts" => Some(s::METRICS),
        "/api/v1/notifications"
        | "/api/v1/notifications/unread-count"
        | "/api/v1/notifications/{id}/read"
        | "/api/v1/notifications/read-all" => Some(s::NOTIFICATIONS),
        "/api/v1/listeners"
        | "/api/v1/listeners/{id}"
        | "/api/v1/listeners/{id}/start"
        | "/api/v1/listeners/{id}/stop" => Some(s::LISTENERS),
        "/api/v1/forward/exec" => Some(s::FORWARD),
        "/api/v1/proxies" | "/api/v1/proxies/{id}" => Some(s::PROXY),
        "/api/v1/ir/scan"
        | "/api/v1/ir/memscan"
        | "/api/v1/ir/memscan/stream"
        | "/api/v1/ir/cache"
        | "/api/v1/ir/fs-timeline"
        | "/api/v1/ir/evidence"
        | "/api/v1/ir/autorun-action"
        | "/api/v1/ir/file-meta"
        | "/api/v1/ir/vt"
        | "/api/v1/ir/snapshots"
        | "/api/v1/ir/snapshots/compare"
        | "/api/v1/ir/snapshots/{id}" => Some(s::IR),
        "/api/v1/agent-gen" | "/api/v1/agent-gen/{id}" | "/api/v1/agent-gen/{id}/download" => {
            Some(s::AGENT_GEN)
        }
        "/api/v1/audit" => Some(s::AUDIT),
        "/api/v1/skill" | "/api/v1/skill/manifest" => Some(s::SKILL),
        // api-keys / auth 账号：不编目（None → API key 一律拒绝，JWT 不受影响）
        _ => None,
    }
}

/// WS 端点（query param token）用的 scope 校验：认证 + scope 一步到位。
pub async fn verify_scoped_token(
    state: &AppState,
    token: &str,
    scope: &'static str,
) -> Result<Claims, Error> {
    let claims = verify_bearer_token(state, token).await?;
    if !claims.has_scope(scope) {
        return Err(Error::Forbidden(format!(
            "api key is missing the '{scope}' scope"
        )));
    }
    Ok(claims)
}

/// 校验 Bearer token（JWT 或 `helm_` API key），返回统一 Claims。
///
/// HTTP 中间件与 WS 端点（query param token，见 stream.rs / terminal.rs）共用：
/// `helm_` 前缀走 API key 哈希比对，其余按 JWT 解码。
pub async fn verify_bearer_token(state: &AppState, token: &str) -> Result<Claims, Error> {
    if token.starts_with(crate::application::api_key_service::RAW_PREFIX) {
        let svc = crate::application::api_key_service::ApiKeyService::new(state.db.clone());
        let (name, scopes) = svc
            .verify(token)
            .await?
            .ok_or_else(|| Error::Unauthorized("invalid token".into()))?;
        Ok(crate::application::api_key_service::claims_for(
            &name, scopes,
        ))
    } else {
        let auth = AuthService::new(state.db.clone(), state.jwt_secret.clone());
        auth.verify(token)
    }
}

fn unauthorized(msg: &str) -> Response {
    (StatusCode::UNAUTHORIZED, Json(json!({ "error": msg }))).into_response()
}

/// scope 拒绝的 403：带上缺失的 scope，方便 AI/调用方理解该找管理员签发什么。
fn forbidden(required: Option<&'static str>, msg: &str) -> Response {
    let body = json!({
        "error": msg,
        "required_scope": required,
    });
    (StatusCode::FORBIDDEN, Json(body)).into_response()
}
