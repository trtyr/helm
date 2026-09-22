//! 证书服务：自签 CA + 签发 Agent 证书（mTLS 底座）。
//!
//! 设计（open-questions #9 落地 + forward mTLS 扩展）：
//! - Server 启动时生成自签 CA + 自身 server 证书（CA 签发）。
//! - 反向模式：Agent 首次用 token 经 HTTP 提交 CSR，Server 用 CA 签发后返回证书 PEM。
//! - 正向模式：管理员用 `helm-server --issue-cert` 离线签发 agent 三件套，
//!   手动预置到 agent `--cert-dir`（agent 不回连，见 real-machine-test-report）。
//! - 配置 `--tls-dir` 后 CA/server 证书持久化（重启不变，预置的 agent 证书持续有效）；
//!   未配置时 CA 每次启动重建（证书轮换即重启），Agent 证书失效后会自动重新换证书。

use anyhow::{Context, Result};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, DnType,
    ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use std::net::IpAddr;
use std::path::PathBuf;
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
        let (issuer, ca_cert_pem, _ca_key_pem) = generate_ca()?;
        let (server_cert_pem, server_key_pem) = generate_leaf_cert(&issuer, server_name)?;
        Ok(Self {
            enabled,
            issuer: Arc::new(issuer),
            ca_cert_pem,
            server_cert_pem,
            server_key_pem,
        })
    }

    /// 从 `tls_dir` 加载或生成 CA/server 证书并持久化（mTLS 部署推荐）。
    ///
    /// `tls_dir` 为空时退化为 [`Self::generate`]（内存 CA，重启轮换）。
    pub fn load_or_generate(tls_dir: &str, server_name: &str, enabled: bool) -> Result<Self> {
        if tls_dir.is_empty() {
            return Self::generate(server_name, enabled);
        }
        let dir = PathBuf::from(tls_dir);
        std::fs::create_dir_all(&dir).with_context(|| format!("create tls dir {tls_dir}"))?;
        let ca_cert_path = dir.join("ca.pem");
        let ca_key_path = dir.join("ca-key.pem");

        let (issuer, ca_cert_pem) = if ca_cert_path.exists() && ca_key_path.exists() {
            // 从持久化的 CA 重建 issuer（server 证书按需重签）
            let cert_pem = std::fs::read_to_string(&ca_cert_path)?;
            let key_pem = std::fs::read_to_string(&ca_key_path)?;
            let key = KeyPair::from_pem(&key_pem)?;
            let issuer = Issuer::from_ca_cert_pem(&cert_pem, key)?;
            tracing::info!(tls_dir = %tls_dir, "loaded persisted CA");
            (issuer, cert_pem)
        } else {
            let (issuer, ca_cert_pem, ca_key_pem) = generate_ca()?;
            std::fs::write(&ca_cert_path, &ca_cert_pem)?;
            write_private_key(&ca_key_path, &ca_key_pem)?;
            tracing::info!(tls_dir = %tls_dir, "generated and persisted new CA");
            (issuer, ca_cert_pem)
        };

        let server_cert_path = dir.join("server.pem");
        let server_key_path = dir.join("server-key.pem");
        let (server_cert_pem, server_key_pem) =
            if server_cert_path.exists() && server_key_path.exists() {
                (
                    std::fs::read_to_string(&server_cert_path)?,
                    std::fs::read_to_string(&server_key_path)?,
                )
            } else {
                let (cert_pem, key_pem) = generate_leaf_cert(&issuer, server_name)?;
                std::fs::write(&server_cert_path, &cert_pem)?;
                write_private_key(&server_key_path, &key_pem)?;
                (cert_pem, key_pem)
            };

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
    ///
    /// **P006 P0-2（安全）**：**绝不**复用 CSR 自带的 params（旧实现是
    /// `csr.signed_by(&issuer)`）。rcgen 的 `from_der` 会把 CSR 里**请求**的
    /// BasicConstraints / KeyUsage / EKU / SAN 全部搬进 `csr.params`，于是任何持有共享
    /// token 的调用方都能递一张 `BasicConstraints: CA:TRUE + KeyCertSign` 的 CSR，
    /// 换到一张**由本 CA 签发的 CA 证书**——拿到它就能给任意名字签证书（完全提权）。
    ///
    /// 现在只取 CSR 的**公钥**（其自签名已由 `from_pem` 校验），其余属性一律由服务端重建：
    /// 主体 CN = 调用方已校验过的 `agent_id`，`CA:FALSE`，只要 `digitalSignature`，
    /// EKU 仅 `clientAuth`，有效期由服务端给定（不复用 CSR 的 `not_after`）。
    pub fn sign_csr(&self, agent_id: &str, csr_pem: &str) -> Result<String> {
        let csr = CertificateSigningRequestParams::from_pem(csr_pem)?;
        let params = agent_cert_params(agent_id)?;
        let cert = params.signed_by(&csr.public_key, &self.issuer)?;
        Ok(cert.pem())
    }

    /// 读取 CSR 的主体 CN（A3：用于把「申请者自报的 agent_id」与 CSR 绑定）。
    ///
    /// 返回 `None` = CSR 未写 CN（合法但不具备绑定信息，调用方自行决定是否放行）。
    pub fn csr_common_name(&self, csr_pem: &str) -> Result<Option<String>> {
        let csr = CertificateSigningRequestParams::from_pem(csr_pem)?;
        let dn = &csr.params.distinguished_name;
        let Some(value) = dn.get(&DnType::CommonName) else {
            return Ok(None);
        };
        let s = match value {
            rcgen::DnValue::PrintableString(v) => v.as_str().to_string(),
            rcgen::DnValue::Ia5String(v) => v.as_str().to_string(),
            rcgen::DnValue::Utf8String(v) => v.clone(),
            // 罕见编码（BMP/Teletex/Universal）：尽量给出可读形态，不做绑定判定以外的假设
            other => format!("{other:?}"),
        };
        Ok(Some(s))
    }

    /// 离线签发 agent 证书三件套（cert/key/ca）写入 `out_dir`，供 forward 模式预置。
    ///
    /// `san` 为逗号分隔的 DNS 或 IP（如 `localhost,43.163.80.102`）。
    pub fn issue_agent_cert(&self, agent_id: &str, san: &str, out_dir: &str) -> Result<()> {
        let dir = PathBuf::from(out_dir);
        std::fs::create_dir_all(&dir).with_context(|| format!("create cert out dir {out_dir}"))?;

        let sans = parse_sans(san)?;
        let mut params = CertificateParams::new(Vec::<String>::new())?;
        params.subject_alt_names = sans;
        params.distinguished_name.push(DnType::CommonName, agent_id);
        params.use_authority_key_identifier_extension = true;
        params.key_usages.push(KeyUsagePurpose::DigitalSignature);
        // agent 证书双向使用：forward 时作 server 证书，reverse 时作 client 证书
        params
            .extended_key_usages
            .push(ExtendedKeyUsagePurpose::ServerAuth);
        params
            .extended_key_usages
            .push(ExtendedKeyUsagePurpose::ClientAuth);
        params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
        params.not_after = OffsetDateTime::now_utc() + Duration::days(LEAF_VALIDITY_DAYS);

        let key = KeyPair::generate()?;
        let cert = params.signed_by(&key, &self.issuer)?;

        std::fs::write(dir.join("cert.pem"), cert.pem())?;
        write_private_key(&dir.join("key.pem"), key.serialize_pem())?;
        std::fs::write(dir.join("ca.pem"), &self.ca_cert_pem)?;
        tracing::info!(agent_id = %agent_id, out_dir = %out_dir, "agent cert issued");
        Ok(())
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

/// 解析逗号分隔的 SAN 列表（IP 或 DNS）。
fn parse_sans(san: &str) -> Result<Vec<SanType>> {
    let mut out = Vec::new();
    for part in san.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if let Ok(ip) = part.parse::<IpAddr>() {
            out.push(SanType::IpAddress(ip));
        } else {
            out.push(SanType::DnsName(part.to_string().try_into()?));
        }
    }
    if out.is_empty() {
        out.push(SanType::DnsName("localhost".to_string().try_into()?));
    }
    Ok(out)
}

/// 生成自签 CA，返回 (issuer, ca_cert_pem, ca_key_pem)。
fn generate_ca() -> Result<(Issuer<'static, KeyPair>, String, String)> {
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
    let ca_key_pem = key_pair.serialize_pem();
    let cert = params.self_signed(&key_pair)?;
    Ok((Issuer::new(params, key_pair), cert.pem(), ca_key_pem))
}

/// 用 CA 签发叶子证书（server/agent 通用），返回 (cert_pem, key_pem)。
fn generate_leaf_cert(
    issuer: &Issuer<'static, KeyPair>,
    common_name: &str,
) -> Result<(String, String)> {
    let mut params = CertificateParams::new(vec![common_name.to_string()])?;
    params
        .distinguished_name
        .push(DnType::CommonName, common_name);
    params.use_authority_key_identifier_extension = true;
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ServerAuth);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ClientAuth);
    params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    params.not_after = OffsetDateTime::now_utc() + Duration::days(LEAF_VALIDITY_DAYS);

    let key_pair = KeyPair::generate()?;
    let cert = params.signed_by(&key_pair, issuer)?;
    Ok((cert.pem(), key_pair.serialize_pem()))
}

/// Agent 证书参数（服务端**重建**，不采信 CSR 自报值 —— P006 P0-2）。
///
/// 抽成独立纯函数便于单测钉住不变量：`CA:FALSE`、只要 `digitalSignature`（无 `keyCertSign`）、
/// EKU 仅 `clientAuth`、有效期由常量给定。
fn agent_cert_params(agent_id: &str) -> Result<CertificateParams> {
    let mut params = CertificateParams::new(Vec::<String>::new())?;
    params.distinguished_name.push(DnType::CommonName, agent_id);
    params.is_ca = IsCa::ExplicitNoCa;
    params.use_authority_key_identifier_extension = true;
    params.key_usages.push(KeyUsagePurpose::DigitalSignature);
    params
        .extended_key_usages
        .push(ExtendedKeyUsagePurpose::ClientAuth);
    params.not_before = OffsetDateTime::now_utc() - Duration::days(1);
    params.not_after = OffsetDateTime::now_utc() + Duration::days(LEAF_VALIDITY_DAYS);
    Ok(params)
}

/// 写私钥文件：unix 下强制 `0600`（P006 P0-4）。
///
/// 此前用 `std::fs::write`，权限完全由进程 umask 决定——常见 `022` 即**世界可读**：
/// 同机任何用户、以及任何备份介质都能直接取走私钥。
fn write_private_key(path: &std::path::Path, contents: impl AsRef<[u8]>) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        f.write_all(contents.as_ref())?;
        // 已存在的文件不受 `mode` 影响（O_CREAT 只在新建时生效）：显式再设一次
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)?;
        Ok(())
    }
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
        let cert_pem = svc.sign_csr("agent-1", &csr.pem().unwrap()).unwrap();
        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn sign_invalid_csr_fails() {
        let svc = CertService::generate("localhost", true).unwrap();
        assert!(svc.sign_csr("agent-1", "not a csr").is_err());
    }

    #[test]
    fn agent_cert_params_is_never_a_ca() {
        let p = agent_cert_params("agent-1").unwrap();
        assert!(
            matches!(p.is_ca, IsCa::ExplicitNoCa),
            "agent 证书必须是显式 CA:FALSE"
        );
        assert!(
            !p.key_usages.contains(&KeyUsagePurpose::KeyCertSign),
            "agent 证书不得带 keyCertSign"
        );
        assert_eq!(p.key_usages, vec![KeyUsagePurpose::DigitalSignature]);
        assert_eq!(
            p.extended_key_usages,
            vec![ExtendedKeyUsagePurpose::ClientAuth]
        );
    }

    /// P0-2 回归钉：CSR 自称 CA + keyCertSign + 别人的 SAN，签发的证书**不得**沿用这些属性。
    ///
    /// 断言方式是「与旧实现（直接把 CSR 的 params 交给 issuer 签）的结果不同」——相同即说明
    /// 仍在搬运 CSR 自报值。
    #[test]
    fn sign_csr_ignores_csr_requested_ca_and_sans() {
        let svc = CertService::generate("localhost", true).unwrap();

        let key = KeyPair::generate().unwrap();
        let mut evil = CertificateParams::new(vec!["evil.example".to_string()]).unwrap();
        evil.distinguished_name.push(DnType::CommonName, "agent-1");
        evil.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        evil.key_usages.push(KeyUsagePurpose::KeyCertSign);
        let csr_pem = evil.serialize_request(&key).unwrap().pem().unwrap();

        let ours = svc.sign_csr("agent-1", &csr_pem).unwrap();
        assert!(ours.contains("BEGIN CERTIFICATE"));

        let legacy = CertificateSigningRequestParams::from_pem(&csr_pem)
            .unwrap()
            .signed_by(&svc.issuer)
            .unwrap()
            .pem();
        assert_ne!(
            ours, legacy,
            "签发的证书不得复用 CSR 自带的 BasicConstraints/KeyUsage/SAN"
        );
    }

    /// P0-4 回归钉：私钥落盘必须 0600。
    #[cfg(unix)]
    #[test]
    fn private_key_is_written_with_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = std::env::temp_dir().join(format!("helm-keyperm-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("key.pem");
        write_private_key(&path, "-----BEGIN PRIVATE KEY-----\n").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "私钥权限应为 0600，实际 {mode:o}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn disabled_service_has_no_tls() {
        let svc = CertService::generate("localhost", false).unwrap();
        assert!(!svc.enabled());
    }

    #[test]
    fn load_or_generate_persists_ca_across_restart() {
        let dir = std::env::temp_dir().join(format!("helm-tls-test-{}", std::process::id()));

        let first =
            CertService::load_or_generate(dir.to_str().unwrap(), "localhost", true).unwrap();
        // 离线签发 agent 三件套
        let out = dir.join("agent-out");
        first
            .issue_agent_cert("agent-1", "localhost,127.0.0.1", out.to_str().unwrap())
            .unwrap();
        for f in ["cert.pem", "key.pem", "ca.pem"] {
            assert!(out.join(f).exists(), "missing {f}");
        }
        let agent_ca = std::fs::read_to_string(out.join("ca.pem")).unwrap();
        assert_eq!(
            agent_ca,
            first.ca_cert_pem(),
            "agent ca 必须与 server CA 一致"
        );

        // 模拟重启：CA/server 证书从磁盘恢复（不变），且新签发兼容
        let second =
            CertService::load_or_generate(dir.to_str().unwrap(), "localhost", true).unwrap();
        assert_eq!(
            first.ca_cert_pem(),
            second.ca_cert_pem(),
            "CA 重启后必须稳定"
        );
        assert_eq!(first.server_cert_pem(), second.server_cert_pem());

        // 持久化 CA 仍能签发合法 agent 证书
        let cert = second
            .sign_csr("agent-2", &{
                let mut params = CertificateParams::new(vec!["agent-2".to_string()]).unwrap();
                params
                    .distinguished_name
                    .push(DnType::CommonName, "agent-2");
                let key = KeyPair::generate().unwrap();
                params.serialize_request(&key).unwrap().pem().unwrap()
            })
            .unwrap();
        assert!(cert.contains("BEGIN CERTIFICATE"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
