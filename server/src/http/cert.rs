//! 证书签发端点：Agent 用 token 提交 CSR，换 CA 签发的证书（mTLS bootstrap）。

use crate::domain::Error;
use crate::grpc::agent_service::token_matches;
use crate::http::AppState;
use axum::Json;
use axum::extract::State;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug, Deserialize)]
pub struct CertRequest {
    pub agent_id: String,
    pub token: String,
    pub csr_pem: String,
}

/// 签发 Agent 证书：POST /api/v1/agents/cert
///
/// 认证走 body 里的 Agent token（非 JWT），因为 Agent 在 bootstrap 阶段尚无证书。
pub async fn issue_cert(
    State(state): State<AppState>,
    Json(body): Json<CertRequest>,
) -> Result<Json<Value>, Error> {
    if !token_matches(&state.server_token, &body.token) {
        return Err(Error::Unauthorized("invalid token".into()));
    }
    let cert_pem = state
        .cert
        .sign_csr(&body.csr_pem)
        .map_err(|e| Error::InvalidArgument(format!("csr: {e}")))?;
    Ok(Json(json!({
        "agent_id": body.agent_id,
        "cert_pem": cert_pem,
        "ca_cert_pem": state.cert.ca_cert_pem(),
    })))
}
