//! Job 查询端点。

use crate::domain::Error;
use crate::http::AppState;
use crate::store::job_repo::JobRepo;
use axum::Json;
use axum::extract::{Path, State};
use serde_json::{Value, json};
use uuid::Uuid;

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
