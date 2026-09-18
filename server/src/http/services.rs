//! 常驻服务端点：创建 / 列表 / 启停 / 重启 / 日志。

use crate::application::service_service::ServiceService;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::service_repo::ServiceRepo;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// D4：按主机过滤（详情页只看本机常驻服务）
    pub host_id: Option<Uuid>,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

#[derive(Debug, Deserialize)]
pub struct CreateServiceBody {
    pub agent_id: String,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub restart_policy: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateServiceBody {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub restart_policy: String,
}

fn service(state: &AppState) -> ServiceService {
    ServiceService::new(state.db.clone(), state.registry.clone())
}

/// 创建服务：POST /api/v1/services
pub async fn create_service(
    State(state): State<AppState>,
    Json(body): Json<CreateServiceBody>,
) -> Result<Json<Value>, Error> {
    let row = service(&state)
        .create(
            &body.agent_id,
            &body.name,
            &body.command,
            &body.args,
            &body.restart_policy,
        )
        .await?;
    Ok(Json(json!({ "service": row })))
}

/// 列出服务：GET /api/v1/services?page=&limit=&host_id=（D4：可选按主机过滤）
pub async fn list_services(
    State(state): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Value>, Error> {
    if let Some(host_id) = q.host_id {
        let rows = ServiceRepo::new(state.db)
            .list_by_host(host_id, q.limit.max(1))
            .await?;
        return Ok(Json(json!({ "services": rows })));
    }
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let rows = service(&state).list_paged(q.limit.max(1), offset).await?;
    Ok(Json(json!({ "services": rows })))
}

/// 启动服务：POST /api/v1/services/{id}/start
pub async fn start_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).start(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 停止服务：POST /api/v1/services/{id}/stop
pub async fn stop_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).stop(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 重启服务：POST /api/v1/services/{id}/restart
pub async fn restart_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).restart(id).await?;
    Ok(Json(json!({ "ok": true })))
}

/// 查服务日志：GET /api/v1/services/{id}/logs
pub async fn service_logs(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let row = service(&state).get(id).await?;
    Ok(Json(json!({ "log": row.log })))
}

/// 更新服务：PUT /api/v1/services/{id}
pub async fn update_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateServiceBody>,
) -> Result<Json<Value>, Error> {
    let row = service(&state)
        .update(
            id,
            &body.name,
            &body.command,
            &body.args,
            &body.restart_policy,
        )
        .await?;
    Ok(Json(json!({ "service": row })))
}

/// 删除服务：DELETE /api/v1/services/{id}
pub async fn delete_service(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    service(&state).delete(id).await?;
    Ok(Json(json!({ "ok": true })))
}
