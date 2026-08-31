use axum::Json;
use serde_json::{Value, json};

/// 存活/就绪探针。
pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}
