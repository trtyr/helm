use clap::Parser;

/// 集中式运维平台 · Agent 被控端配置。
#[derive(Debug, Clone, Parser)]
#[command(name = "helm-agent", version, about = "集中式运维平台 · Agent")]
pub struct Config {
    /// Agent 唯一标识
    #[arg(long, env = "HELM_AGENT_ID")]
    pub agent_id: String,

    /// Server gRPC 地址（反向模式连入目标）
    #[arg(
        long,
        env = "HELM_SERVER_ADDR",
        default_value = "http://127.0.0.1:50051"
    )]
    pub server_addr: String,

    /// 注册 token
    #[arg(long, env = "HELM_AGENT_TOKEN", default_value = "")]
    pub token: String,

    /// 日志级别
    #[arg(long, env = "HELM_LOG", default_value = "info")]
    pub log_level: String,

    /// 连接模式：reverse（默认，Agent 主动连）| forward（Agent 监听）
    #[arg(long, env = "HELM_CONN_MODE", default_value = "reverse")]
    pub conn_mode: String,

    /// forward 模式监听地址
    #[arg(long, env = "HELM_LISTEN_ADDR", default_value = "0.0.0.0:50052")]
    pub listen_addr: String,

    /// 日志目录（非空则按天滚动落文件，供无控制台的服务模式使用）
    #[arg(long, env = "HELM_LOG_DIR", default_value = "")]
    pub log_dir: String,

    /// mTLS server 证书 SAN 名（默认 localhost，用于证书校验）
    #[arg(long, env = "HELM_TLS_SERVER_NAME", default_value = "localhost")]
    pub tls_server_name: String,

    /// 证书缓存目录（非空则启用 mTLS，证书/密钥/CA 落此目录）
    #[arg(long, env = "HELM_CERT_DIR", default_value = "")]
    pub cert_dir: String,

    /// Server HTTP 地址（换证书用，默认空则从 server_addr 推导 http://host:18080）
    #[arg(long, env = "HELM_SERVER_HTTP_ADDR", default_value = "")]
    pub server_http_addr: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::parse())
    }
}
