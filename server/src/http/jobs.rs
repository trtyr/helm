//! Job 查询端点。

use crate::http::AppState;
use crate::store::job_repo::JobRepo;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde_json::{Value, json};
use uuid::Uuid;

/// 查询 Job：GET /api/v1/jobs/{id}
pub async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let job = JobRepo::new(state.db)
        .get(id)
        .await
        .map_err(|e| internal(e.to_string()))?;
    match job {
        Some(j) => Ok(Json(json!({ "job": j }))),
        None => Err((
            StatusCode::NOT_FOUND,
            Json(json!({ "error": "job not found" })),
        )),
    }
}

fn internal(msg: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "error": msg })),
    )
}
