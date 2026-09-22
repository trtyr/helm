//! 证书签发端点：Agent 用 token 提交 CSR，换 CA 签发的证书（mTLS bootstrap）。

use crate::application::audit_service::AuditService;
use crate::domain::Error;
use crate::grpc::agent_service::token_matches_any;
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
    if !token_matches_any(&state.server_tokens, &body.token) {
        return Err(Error::Unauthorized("invalid token".into()));
    }

    // A3：CSR 的 CN 必须与请求里的 agent_id 一致。
    // 修复的攻击路径：持共享 token 者可为**任意 agent_id** 领取 CA 签发的合法证书
    // → 此后 mTLS 只证明「持有 CA 签发证书」，不证明「是本台主机」；结合同 id 顶号（E3）
    // 可接管现有连接。绑定后：证书身份 = agent_id。
    match state.cert.csr_common_name(&body.csr_pem) {
        Ok(Some(cn)) if cn != body.agent_id => {
            // 签发拒绝必须留痕（此前签发动作完全不入审计）
            AuditService::new(state.db.clone())
                .record_best_effort(
                    &format!("agent:{}", body.agent_id),
                    "agent_cert_denied",
                    &body.agent_id,
                    json!({ "reason": "cn_mismatch", "csr_cn": cn }),
                )
                .await;
            return Err(Error::Forbidden(format!(
                "CSR CN '{cn}' does not match agent_id '{}'",
                body.agent_id
            )));
        }
        Ok(None) => {
            // 无 CN 的 CSR：不具备绑定信息。**拒绝**而不是放行——否则绑定可被「不写 CN」绕过。
            AuditService::new(state.db.clone())
                .record_best_effort(
                    &format!("agent:{}", body.agent_id),
                    "agent_cert_denied",
                    &body.agent_id,
                    json!({ "reason": "missing_cn" }),
                )
                .await;
            return Err(Error::Forbidden(
                "CSR must carry subject CN equal to agent_id".into(),
            ));
        }
        Ok(Some(_)) => {}
        Err(e) => return Err(Error::InvalidArgument(format!("csr: {e}"))),
    }

    let cert_pem = state
        .cert
        .sign_csr(&body.agent_id, &body.csr_pem)
        .map_err(|e| Error::InvalidArgument(format!("csr: {e}")))?;
    // A3：签发成功入审计（谁在什么时候为哪个 agent_id 领了证书）
    AuditService::new(state.db.clone())
        .record_best_effort(
            &format!("agent:{}", body.agent_id),
            "agent_cert_issued",
            &body.agent_id,
            json!({ "csr_cn": body.agent_id }),
        )
        .await;
    Ok(Json(json!({
        "agent_id": body.agent_id,
        "cert_pem": cert_pem,
        "ca_cert_pem": state.cert.ca_cert_pem(),
    })))
}
