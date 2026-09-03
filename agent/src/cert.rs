//! mTLS 证书管理：首次用 token 换证书 + 本地缓存。

use anyhow::{Context, Result, bail};
use rcgen::{CertificateParams, DnType, KeyPair};
use std::path::PathBuf;

/// Agent 持有的证书三元组（cert / key / ca）。
#[derive(Clone)]
pub struct AgentCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
}

/// 读取本地预置的证书三件套（forward 模式专用；缺任一即报错提示预置）。
pub fn load_cached(cert_dir: &str) -> Result<AgentCert> {
    let dir = PathBuf::from(cert_dir);
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    let ca_path = dir.join("ca.pem");
    if !(cert_path.exists() && key_path.exists() && ca_path.exists()) {
        bail!(
            "forward 模式 mTLS 需要预置证书：在 Server 上执行 \
             `helm-server --issue-cert --issue-agent-id <id> --issue-san <san> \
             --issue-out-dir <dir> --tls-dir <dir>`，把生成的 cert.pem/key.pem/ca.pem \
             复制到 {cert_dir}"
        );
    }
    Ok(AgentCert {
        cert_pem: std::fs::read_to_string(&cert_path)?,
        key_pem: std::fs::read_to_string(&key_path)?,
        ca_pem: std::fs::read_to_string(&ca_path)?,
    })
}

/// 获取或申请证书：优先本地缓存，否则生成 key + CSR，用 token 经 HTTP 换证书。
pub async fn obtain(
    cert_dir: &str,
    agent_id: &str,
    token: &str,
    server_http_addr: &str,
) -> Result<AgentCert> {
    let dir = PathBuf::from(cert_dir);
    std::fs::create_dir_all(&dir)?;
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    let ca_path = dir.join("ca.pem");

    // 有缓存直接用（证书轮换即重启后重新申请）
    if cert_path.exists() && key_path.exists() && ca_path.exists() {
        return Ok(AgentCert {
            cert_pem: std::fs::read_to_string(&cert_path)?,
            key_pem: std::fs::read_to_string(&key_path)?,
            ca_pem: std::fs::read_to_string(&ca_path)?,
        });
    }

    // 生成 key + CSR（私钥留本地）
    let mut params = CertificateParams::new(vec![agent_id.to_string()])?;
    params.distinguished_name.push(DnType::CommonName, agent_id);
    let key = KeyPair::generate()?;
    let csr = params.serialize_request(&key)?;
    let csr_pem = csr.pem()?;

    // POST 换证书（token 认证）
    let url = format!("{server_http_addr}/api/v1/agents/cert");
    let resp = reqwest::Client::new()
        .post(&url)
        .json(&serde_json::json!({
            "agent_id": agent_id,
            "token": token,
            "csr_pem": csr_pem,
        }))
        .send()
        .await
        .with_context(|| format!("request cert from {url}"))?;
    if !resp.status().is_success() {
        bail!("cert request failed: {}", resp.status());
    }
    let body: serde_json::Value = resp.json().await?;
    let cert_pem = body["cert_pem"]
        .as_str()
        .context("missing cert_pem")?
        .to_string();
    let ca_pem = body["ca_cert_pem"]
        .as_str()
        .context("missing ca_cert_pem")?
        .to_string();
    let key_pem = key.serialize_pem();

    std::fs::write(&cert_path, &cert_pem)?;
    std::fs::write(&key_path, &key_pem)?;
    std::fs::write(&ca_path, &ca_pem)?;

    Ok(AgentCert {
        cert_pem,
        key_pem,
        ca_pem,
    })
}
