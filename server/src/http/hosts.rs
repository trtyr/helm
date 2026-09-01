//! 主机查询与创建端点。

use crate::domain::Error;
use crate::http::AppState;
use crate::store::host_repo::{HostRepo, NewHost};
use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

/// 创建主机参数。
#[derive(Debug, Deserialize)]
pub struct CreateHostBody {
    pub hostname: String,
    /// reverse（默认，Agent 主动连）| forward（Server 主动连）
    #[serde(default = "default_conn_mode")]
    pub conn_mode: String,
    /// forward 模式的拨号地址（如 100.80.65.64:50052）
    #[serde(default)]
    pub addr: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_conn_mode() -> String {
    "reverse".to_string()
}

/// 列出主机：GET /api/v1/hosts
pub async fn list_hosts(State(state): State<AppState>) -> Result<Json<Value>, Error> {
    let hosts = HostRepo::new(state.db).list().await?;
    Ok(Json(json!({ "hosts": hosts })))
}

/// 创建主机：POST /api/v1/hosts
pub async fn create_host(
    State(state): State<AppState>,
    Json(body): Json<CreateHostBody>,
) -> Result<Json<Value>, Error> {
    let conn_mode = if body.conn_mode == "forward" {
        "forward".to_string()
    } else {
        "reverse".to_string()
    };
    let host = HostRepo::new(state.db)
        .insert(&NewHost {
            hostname: body.hostname,
            // forward 模式下目标机细节未知，拨号成功后由 Agent 回报补全
            os: String::new(),
            arch: String::new(),
            platform: String::new(),
            tags: body.tags,
            conn_mode,
            addr: body.addr,
        })
        .await?;
    Ok(Json(json!({ "host": host })))
}
