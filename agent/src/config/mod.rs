use clap::Parser;

/// 集中式运维平台 · Agent 被控端配置。
///
/// server_addr / token / agent_id / conn_mode / listen_addr 支持编译期烙入（生成器编译时注入 HELM_BAKE_* 环境变量，
/// 见 agent/build.rs），运行时优先级：CLI 参数 > 环境变量 > 编译期烙入值 > 内置兜底。
/// agent_id 兜底为主机名——同一二进制可直接拷贝到任意主机运行上线。
#[derive(Debug, Clone, Parser)]
#[command(name = "helm-agent", version, about = "集中式运维平台 · Agent")]
pub struct Config {
    /// Agent 唯一标识（缺省取烙入值，再退回主机名）
    #[arg(long, env = "HELM_AGENT_ID", default_value = "")]
    pub agent_id: String,

    /// Server gRPC 地址（反向模式连入目标）
    #[arg(long, env = "HELM_SERVER_ADDR", default_value = "")]
    pub server_addr: String,

    /// 注册 token
    #[arg(long, env = "HELM_AGENT_TOKEN", default_value = "")]
    pub token: String,

    /// 日志级别
    #[arg(long, env = "HELM_LOG", default_value = "info")]
    pub log_level: String,

    /// 连接模式：reverse（默认，Agent 主动连）| forward（Agent 监听）
    #[arg(long, env = "HELM_CONN_MODE", default_value = "")]
    pub conn_mode: String,

    /// forward 模式监听地址
    #[arg(long, env = "HELM_LISTEN_ADDR", default_value = "")]
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

    /// Server HTTP 地址（换证书用，默认空则从 server_addr 推导 http://host:8080）
    #[arg(long, env = "HELM_SERVER_HTTP_ADDR", default_value = "")]
    pub server_http_addr: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let mut cfg = Self::parse();

        if cfg.server_addr.is_empty() {
            cfg.server_addr = option_env!("HELM_BAKE_SERVER_ADDR")
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "http://127.0.0.1:50051".to_string());
        }
        if cfg.token.is_empty() {
            cfg.token = option_env!("HELM_BAKE_AGENT_TOKEN")
                .unwrap_or("")
                .to_string();
        }
        if cfg.conn_mode.is_empty() {
            cfg.conn_mode = option_env!("HELM_BAKE_CONN_MODE")
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "reverse".to_string());
        }
        if cfg.listen_addr.is_empty() {
            cfg.listen_addr = option_env!("HELM_BAKE_LISTEN_ADDR")
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| "0.0.0.0:50052".to_string());
        }
        if cfg.agent_id.is_empty() {
            cfg.agent_id = option_env!("HELM_BAKE_AGENT_ID")
                .filter(|v| !v.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    hostname::get()
                        .map(|h| h.to_string_lossy().to_lowercase().replace(' ', "-"))
                        .unwrap_or_else(|_| "unknown-host".to_string())
                });
        }

        Ok(cfg)
    }
}
