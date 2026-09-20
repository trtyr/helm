use clap::Parser;

/// 集中式运维平台 · 控制端配置。
#[derive(Debug, Clone, Parser)]
#[command(name = "helm-server", version, about = "集中式运维平台 · 控制端")]
pub struct Config {
    /// HTTP 监听地址（控制台 API + health）
    #[arg(long, env = "HELM_HTTP_ADDR", default_value = "0.0.0.0:8080")]
    pub http_addr: String,

    /// 前端静态资源目录（console 构建产物 dist/）：设置后 server 直接托管控制台
    /// （前后端一体化，同一端口）；空 = 不托管（开发模式走 Vite dev server）
    #[arg(long, env = "HELM_WEB_DIST_DIR", default_value = "")]
    pub web_dist_dir: String,

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

    /// Job 超时兜底阈值（秒，默认 300）：running 超此时长由 sweeper 置 timed_out，
    /// queued 孤行超此时长置 failed；0 = 禁用 sweeper
    #[arg(long, env = "HELM_JOB_TIMEOUT_SECS", default_value = "300")]
    pub job_timeout_secs: u64,

    /// 数据保留天数（C1，默认 90）：超过的 jobs / audit_logs / file_transfers 及
    /// 时序类（metrics/alerts/notifications）由后台清理；IR 表不自动清理（取证数据需显式策略）
    #[arg(long, env = "HELM_RETENTION_DAYS", default_value = "90")]
    pub retention_days: i64,

    /// MCP 渐进分层（P002 T4，默认 2）：1=入口（清单不展开，AI 用 catalog 发现）/
    /// 2=host 域（对主机做的一切，ir 带 windows 专属标注）/ 3=全量（含 platform 管理能力）
    #[arg(
        long,
        env = "HELM_MCP_TIER",
        default_value = "2",
        value_parser = clap::value_parser!(u8).range(1..=3)
    )]
    pub mcp_tier: u8,

    /// 离线升级告警阈值（P003 T2，分钟，默认 30）：主机最新事件为 offline 且持续超此
    /// 时长则升级写 alerts（区别于断连即发的抖动通知）；0 = 禁用
    #[arg(long, env = "HELM_OFFLINE_ALERT_MINS", default_value = "30")]
    pub offline_alert_mins: u64,

    /// 数据库连接池容量（C3，默认 10）：按 Agent 数与控制台并发调大
    #[arg(long, env = "HELM_DB_MAX_CONNECTIONS", default_value = "10")]
    pub db_max_connections: u32,

    /// 连接池获取超时（秒，C3，默认 30）：池耗尽时请求等待上限，超时快速失败
    #[arg(long, env = "HELM_DB_ACQUIRE_TIMEOUT", default_value = "30")]
    pub db_acquire_timeout_secs: u64,

    /// 会话空闲超时（秒），无输入/输出超过该时长自动关闭会话（默认 300s）
    #[arg(long, env = "HELM_SESSION_IDLE_TIMEOUT", default_value = "300")]
    pub session_idle_timeout_secs: u64,

    /// mTLS server 证书 SAN 名（默认 localhost，Agent 校验证书用）
    #[arg(long, env = "HELM_TLS_SERVER_NAME", default_value = "localhost")]
    pub tls_server_name: String,

    /// 是否启用 mTLS（Agent↔Server 双向认证）
    #[arg(long, env = "HELM_MTLS", action = clap::ArgAction::SetTrue)]
    pub mtls: bool,

    /// TLS 材料目录（CA/server 证书持久化；mTLS 部署强烈建议，保证重启后 CA 稳定）
    #[arg(long, env = "HELM_TLS_DIR", default_value = "")]
    pub tls_dir: String,

    /// 离线签发 agent 证书三件套后退出（forward 预置分发用，不启动服务）
    #[arg(long, env = "HELM_ISSUE_CERT", action = clap::ArgAction::SetTrue)]
    pub issue_cert: bool,

    /// issue-cert：agent 唯一标识（写入证书 CN）
    #[arg(long, env = "HELM_ISSUE_AGENT_ID", default_value = "")]
    pub issue_agent_id: String,

    /// issue-cert：SAN 列表，逗号分隔 DNS/IP（如 localhost,43.163.80.102）
    #[arg(long, env = "HELM_ISSUE_SAN", default_value = "")]
    pub issue_san: String,

    /// issue-cert：三件套输出目录（cert.pem/key.pem/ca.pem）
    #[arg(long, env = "HELM_ISSUE_OUT_DIR", default_value = "")]
    pub issue_out_dir: String,

    /// Agent 源码工作区目录（现场编译生成 Agent 用；Server 须能在此目录执行 cargo）
    #[arg(long, env = "HELM_AGENT_SOURCE_DIR", default_value = ".")]
    pub agent_source_dir: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        Ok(Self::parse())
    }
}
