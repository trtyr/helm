//! Job 查询端点。

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
}

fn default_page() -> i64 {
    1
}

fn default_limit() -> i64 {
    20
}

/// 列出 Job：GET /api/v1/jobs?page=&limit=
pub async fn list_jobs(
    State(state): State<AppState>,
    Query(q): Query<JobQuery>,
) -> Result<Json<Value>, Error> {
    let offset = (q.page.max(1) - 1) * q.limit.max(1);
    let jobs = JobRepo::new(state.db)
        .list_paged(q.limit.max(1), offset)
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
