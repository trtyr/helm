//! 证书服务：自签 CA + 签发 Agent 证书（mTLS 底座）。
//!
//! 设计（open-questions #9 落地）：
//! - Server 启动时生成自签 CA + 自身 server 证书（CA 签发）。
//! - Agent 首次用 token 经 HTTP 提交 CSR，Server 用 CA 签发后返回证书 PEM。
//! - 之后 Agent 用证书走 gRPC mTLS 双向认证。
//! - CA 每次启动重建（证书轮换即重启），Agent 证书失效后会自动重新换证书。

use anyhow::Result;
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DnType, IsCa, Issuer,
    KeyPair,
};
use std::sync::Arc;
use time::{Duration, OffsetDateTime};

const CA_VALIDITY_DAYS: i64 = 3650; // CA 10 年
const LEAF_VALIDITY_DAYS: i64 = 365; // 叶子证书 1 年

/// 证书服务：持有 CA issuer + CA/server 证书 PEM。
#[derive(Clone)]
pub struct CertService {
    enabled: bool,
    issuer: Arc<Issuer<'static, KeyPair>>,
    ca_cert_pem: String,
    server_cert_pem: String,
    server_key_pem: String,
}

impl CertService {
    /// 生成全新 CA + server 证书。
    pub fn generate(server_name: &str, enabled: bool) -> Result<Self> {
        let (issuer, ca_cert_pem) = generate_ca()?;
        let (server_cert_pem, server_key_pem) = generate_server_cert(&issuer, server_name)?;
        Ok(Self {
            enabled,
            issuer: Arc::new(issuer),
            ca_cert_pem,
            server_cert_pem,
            server_key_pem,
        })
    }

    /// 是否启用 mTLS（gRPC 监听器据此决定是否要求客户端证书）。
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// 签发 Agent CSR，返回证书 PEM。
    pub fn sign_csr(&self, csr_pem: &str) -> Result<String> {
        let csr = CertificateSigningRequestParams::from_pem(csr_pem)?;
        let cert = csr.signed_by(&self.issuer)?;
        Ok(cert.pem())
    }

    pub fn ca_cert_pem(&self) -> &str {
        &self.ca_cert_pem
    }

    pub fn server_cert_pem(&self) -> &str {
        &self.server_cert_pem
    }

    pub fn server_key_pem(&self) -> &str {
        &self.server_key_pem
    }
}

/// 生成自签 CA，返回 (issuer, ca_cert_pem)。
fn generate_ca() -> Result<(Issuer<'static, KeyPair>, String)> {
    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params
        .distinguished_name
        .push(DnType::CommonName, "helm-ca");
    params
        .distinguished_name
        .push(DnType::OrganizationName, "helm");
    params.key_usages.push(rcgen::KeyUsagePurpose::KeyCertSign);
    params.key_usages.push(rcgen::KeyUsagePurpose::CrlSign);
    params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    params.not_after = OffsetDateTime::now_utc() + Duration::days(CA_VALIDITY_DAYS);

    let key_pair = KeyPair::generate()?;
    let cert = params.self_signed(&key_pair)?;
    Ok((Issuer::new(params, key_pair), cert.pem()))
}

/// 用 CA 签发 server 证书，返回 (cert_pem, key_pem)。
fn generate_server_cert(
    issuer: &Issuer<'static, KeyPair>,
    server_name: &str,
) -> Result<(String, String)> {
    let mut params = CertificateParams::new(vec![server_name.to_string()])?;
    params
        .distinguished_name
        .push(DnType::CommonName, server_name);
    params.use_authority_key_identifier_extension = true;
    params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    params.not_after = OffsetDateTime::now_utc() + Duration::days(LEAF_VALIDITY_DAYS);

    let key_pair = KeyPair::generate()?;
    let cert = params.signed_by(&key_pair, issuer)?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_and_sign_roundtrip() {
        let svc = CertService::generate("localhost", true).unwrap();
        assert!(svc.enabled());
        assert!(svc.ca_cert_pem().contains("BEGIN CERTIFICATE"));
        assert!(svc.server_cert_pem().contains("BEGIN CERTIFICATE"));
        assert!(svc.server_key_pem().contains("BEGIN"));

        // Agent 侧生成 key + CSR，Server 签，返回合法证书
        let mut params = CertificateParams::new(vec!["agent-1".to_string()]).unwrap();
        params
            .distinguished_name
            .push(DnType::CommonName, "agent-1");
        let key = KeyPair::generate().unwrap();
        let csr = params.serialize_request(&key).unwrap();
        let cert_pem = svc.sign_csr(&csr.pem().unwrap()).unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn sign_invalid_csr_fails() {
        let svc = CertService::generate("localhost", true).unwrap();
        assert!(svc.sign_csr("not a csr").is_err());
    }

    #[test]
    fn disabled_service_has_no_tls() {
        let svc = CertService::generate("localhost", false).unwrap();
        assert!(!svc.enabled());
    }
}
