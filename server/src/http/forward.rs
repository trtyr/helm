//! 正向连接端点。

use crate::application::forward_service::ForwardService;
use crate::domain::Error;
use crate::http::AppState;
use crate::store::host_repo::HostRepo;
use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct ForwardBody {
    /// 按主机名查拨号地址（优先）
    #[serde(default)]
    pub hostname: Option<String>,
    /// 或直接指定拨号地址
    #[serde(default)]
    pub agent_addr: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// 正向连接执行命令：POST /api/v1/forward/exec
pub async fn exec(
    State(state): State<AppState>,
    Json(body): Json<ForwardBody>,
) -> Result<Json<Value>, Error> {
    let addr = match (body.hostname, body.agent_addr) {
        (Some(name), _) => {
            let host = HostRepo::new(state.db)
                .get_by_hostname(&name)
                .await?
                .ok_or_else(|| Error::NotFound(format!("host: {name}")))?;
            if host.conn_mode != "forward" {
                return Err(Error::InvalidArgument(format!(
                    "host '{name}' is not forward mode"
                )));
            }
            if host.addr.is_empty() {
                return Err(Error::InvalidArgument(format!("host '{name}' has no addr")));
            }
            host.addr
        }
        (None, Some(addr)) => addr,
        (None, None) => return Err(Error::InvalidArgument("need hostname or agent_addr".into())),
    };

    let service = ForwardService::new();
    let (output, exit_code) = service.exec(&addr, &body.command, &body.args).await?;
    Ok(Json(json!({
        "output": output,
        "exit_code": exit_code,
    })))
}
