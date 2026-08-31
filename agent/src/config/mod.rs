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
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::parse())
    }
}
