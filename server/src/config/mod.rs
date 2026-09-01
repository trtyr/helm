use clap::Parser;

/// 集中式运维平台 · 控制端配置。
#[derive(Debug, Clone, Parser)]
#[command(name = "helm-server", version, about = "集中式运维平台 · 控制端")]
pub struct Config {
    /// HTTP 监听地址（控制台 API + health）
    #[arg(long, env = "HELM_HTTP_ADDR", default_value = "0.0.0.0:8080")]
    pub http_addr: String,

    /// gRPC 监听地址（Agent 反向连入）
    #[arg(long, env = "HELM_GRPC_ADDR", default_value = "0.0.0.0:50051")]
    pub grpc_addr: String,

    /// 数据库连接串（Postgres）
    #[arg(
        long,
        env = "HELM_DATABASE_URL",
        default_value = "postgres://helm:helm@localhost:5433/helm"
    )]
    pub database_url: String,

    /// Server 端 Agent 认证 token（生产必须配置强随机值）
    #[arg(long, env = "HELM_SERVER_TOKEN", default_value = "dev-token-change-me")]
    pub server_token: String,

    /// JWT 签名密钥（生产必须配置强随机值）
    #[arg(long, env = "HELM_JWT_SECRET", default_value = "dev-secret-change-me")]
    pub jwt_secret: String,

    /// 日志级别
    #[arg(long, env = "HELM_LOG", default_value = "info")]
    pub log_level: String,

    /// 心跳超时判定阈值（秒），超过视为离线（默认 30s = 3×心跳间隔）
    #[arg(long, env = "HELM_HEARTBEAT_TIMEOUT", default_value = "30")]
    pub heartbeat_timeout_secs: u64,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::parse())
    }
}
