//! API key 管理端点（决策 010：机器对机器认证）。
//!
//! 仅 JWT 可操作（管理面不开放给 key 自身，防止 key 自我续期/扩散）；
//! 明文 key 只在创建响应出现一次，列表/详情只回展示前缀。

use crate::application::api_key_service::{ApiKeyService, ROLE_API_KEY};
use crate::application::audit_service::AuditService;
use crate::application::auth_service::Claims;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::api_key_repo::ApiKeyRow;
use axum::Json;
use axum::extract::{Extension, Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

/// 对外视图：不含 `key_hash`。
#[derive(Debug, serde::Serialize)]
pub struct ApiKeyView {
    pub id: Uuid,
    pub name: String,
    pub prefix: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_used_at: Option<chrono::DateTime<chrono::Utc>>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<ApiKeyRow> for ApiKeyView {
    fn from(r: ApiKeyRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            prefix: r.prefix,
            created_at: r.created_at,
            last_used_at: r.last_used_at,
            expires_at: r.expires_at,
            revoked_at: r.revoked_at,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    pub name: String,
    /// RFC 3339 过期时间（如 2026-12-31T23:59:59Z），空则永不过期。
    pub expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    50
}

/// API key 不能管理 API key（自管 = 自我续期/扩散面）。
fn require_jwt(claims: &Claims) -> Result<(), Error> {
    if claims.role == ROLE_API_KEY {
        return Err(Error::Forbidden(
            "api keys cannot manage api keys; login with JWT".into(),
        ));
    }
    Ok(())
}

/// 创建：POST /api/v1/api-keys
pub async fn create(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(body): Json<CreateBody>,
) -> Result<Json<Value>, Error> {
    require_jwt(&claims)?;
    let name = body.name.trim();
    if name.is_empty() {
        return Err(Error::InvalidArgument("name must not be empty".into()));
    }

    let expires_at = match body.expires_at.as_deref() {
        None | Some("") => None,
        Some(s) => Some(
            chrono::DateTime::parse_from_rfc3339(s)
                .map_err(|_| Error::InvalidArgument(format!("expires_at not RFC 3339: {s}")))?
                .with_timezone(&chrono::Utc),
        ),
    };

    let (row, raw) = ApiKeyService::new(state.db.clone())
        .create(name, expires_at)
        .await?;

    let _ = AuditService::new(state.db.clone())
        .record(
            &claims.sub,
            "api_key_create",
            &row.id.to_string(),
            json!({ "name": row.name, "prefix": row.prefix }),
        )
        .await;

    // 明文 key 仅此一次返回
    Ok(Json(
        json!({ "api_key": ApiKeyView::from(row), "key": raw }),
    ))
}

/// 列出：GET /api/v1/api-keys?page=&limit=
pub async fn list(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    require_jwt(&claims)?;
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let rows = ApiKeyService::new(state.db.clone())
        .list_paged(q.limit.max(1), offset)
        .await?;
    let keys: Vec<ApiKeyView> = rows.into_iter().map(ApiKeyView::from).collect();
    Ok(Json(json!({ "api_keys": keys })))
}

/// 详情：GET /api/v1/api-keys/{id}
pub async fn get_one(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    require_jwt(&claims)?;
    let row = ApiKeyService::new(state.db.clone())
        .get(id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("api key: {id}")))?;
    Ok(Json(json!({ "api_key": ApiKeyView::from(row) })))
}

/// 吊销：DELETE /api/v1/api-keys/{id}（幂等：已吊销仍返回 ok）
pub async fn revoke(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    require_jwt(&claims)?;
    let svc = ApiKeyService::new(state.db.clone());
    svc.get(id)
        .await?
        .ok_or_else(|| Error::NotFound(format!("api key: {id}")))?;
    svc.revoke(id).await?;

    let _ = AuditService::new(state.db.clone())
        .record(&claims.sub, "api_key_revoke", &id.to_string(), json!({}))
        .await;

    Ok(Json(json!({ "ok": true })))
}
