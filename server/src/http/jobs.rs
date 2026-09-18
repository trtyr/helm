//! Job 查询与取消端点。

use crate::application::exec_service::ExecService;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::job_repo::JobRepo;
use axum::Json;
use axum::extract::{Path, Query, State};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

#[derive(Debug, Deserialize)]
pub struct JobQuery {
    #[serde(default = "default_page")]
    pub page: i64,
    #[serde(default = "default_limit")]
    pub limit: i64,
    /// D4：按状态过滤（queued/running/succeeded/failed/cancelled/timed_out）
    pub status: Option<String>,
    /// D4：按主机过滤
    pub host_id: Option<Uuid>,
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

/// 列出 Job：GET /api/v1/jobs?page=&limit=&status=&host_id=（D4：可选过滤）
pub async fn list_jobs(
    State(state): State<AppState>,
    Query(q): Query<JobQuery>,
) -> Result<Json<Value>, Error> {
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let jobs = JobRepo::new(state.db)
        .list_filtered(q.status.as_deref(), q.host_id, q.limit.max(1), offset)
        .await?;
    Ok(Json(json!({ "jobs": jobs })))
}

/// 查询 Job：GET /api/v1/jobs/{id}
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let job = JobRepo::new(state.db).get(id).await?;
    match job {
        Some(j) => Ok(Json(json!({ "job": j }))),
        None => Err(Error::NotFound(format!("job: {id}"))),
    }
}

/// 取消 Job：POST /api/v1/jobs/{id}/cancel（EN-64）。
///
/// queued → 直接收敛为 cancelled；running → 在线 agent 下发 JobCancel 真中断，
/// 离线则置 cancelled 并记补偿（重连后补杀目标机残留进程）。
/// 终态 job 返回 400（invalid_argument）。
pub async fn cancel_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, Error> {
    let service = ExecService::new(state.db, state.registry);
    let (cancelled, delivered, compensated) = service.cancel(id).await?;
    Ok(Json(json!({
        "cancelled": cancelled,
        "delivered": delivered,
        "compensated": compensated,
    })))
}
