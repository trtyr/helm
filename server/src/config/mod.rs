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

    /// 额外可接受的 Agent token（逗号分隔，A2 轮换用）：新旧 token 并存一段时间，
    /// Agent 分批改配置后移除旧的即可实现**不停机轮换**；空 = 只认 HELM_SERVER_TOKEN
    #[arg(long = "server-tokens", env = "HELM_SERVER_TOKENS", default_value = "")]
    pub server_tokens_extra: String,

    /// 拒绝以弱默认凭据启动（A1）：置 1 时若仍在使用默认 token/密钥则**启动失败**；
    /// 默认只打 ERROR 日志（开发/CI 与 e2e 脚本依赖默认值，硬失败会把它们一起打断）
    #[arg(long, env = "HELM_REQUIRE_STRONG_DEFAULTS", action = clap::ArgAction::SetTrue)]
    pub require_strong_defaults: bool,

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

    /// 日志目录（P003 T3，默认 ./logs）：server 日志按天轮转双写（stdout + 文件），
    /// 保留天数复用 retention_days；置空 = 仅 stdout 不落盘
    #[arg(long, env = "HELM_LOG_DIR", default_value = "./logs")]
    pub log_dir: String,

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

    /// 可接受的 Agent token 全集：主 token + `HELM_SERVER_TOKENS` 里的轮换 token（去重、去空）。
    ///
    /// 顺序无关；空集合时**拒绝一切 Agent 注册**（fail-closed，见 `token_matches_any`）。
    pub fn accepted_server_tokens(&self) -> Vec<String> {
        let mut out = Vec::new();
        let mut push = |t: &str| {
            let t = t.trim();
            if !t.is_empty() && !out.iter().any(|x: &String| x == t) {
                out.push(t.to_string());
            }
        };
        push(&self.server_token);
        for t in self.server_tokens_extra.split(',') {
            push(t);
        }
        out
    }

    /// A1：仍在使用的**弱默认凭据**清单（空 = 无问题）。
    ///
    /// 判定用「等于出厂默认值」而非熵估计——目的是拦住「忘了改」而不是评估强度。
    pub fn insecure_defaults(&self) -> Vec<&'static str> {
        let mut weak = Vec::new();
        if self.server_token.trim() == "dev-token-change-me" {
            weak.push("HELM_SERVER_TOKEN");
        }
        if self.jwt_secret.trim() == "dev-secret-change-me" {
            weak.push("HELM_JWT_SECRET");
        }
        if self
            .accepted_server_tokens()
            .iter()
            .any(|t| t == "dev-token-change-me")
        {
            weak.push("HELM_SERVER_TOKENS(含出厂默认值)");
        }
        weak
    }

    /// A1：启动期弱值守卫——命中即 ERROR；`HELM_REQUIRE_STRONG_DEFAULTS=1` 时拒绝启动。
    ///
    /// **语义变更（2026-09-20，T5）**：此前弱默认值完全静默，现在是启动期可见的 ERROR。
    pub fn guard_insecure_defaults(&self) -> anyhow::Result<()> {
        let weak = self.insecure_defaults();
        if weak.is_empty() {
            return Ok(());
        }
        tracing::error!(
            weak = ?weak,
            "insecure default credentials in use — set strong random values before exposing this server"
        );
        if self.require_strong_defaults {
            anyhow::bail!(
                "HELM_REQUIRE_STRONG_DEFAULTS=1 但仍在用弱默认凭据: {}",
                weak.join(", ")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Config;
    use clap::Parser;

    /// P003 T2/T3 契约：默认值与文档口径一致（HELM_OFFLINE_ALERT_MINS=30、HELM_LOG_DIR=./logs）。
    #[test]
    fn defaults_match_docs() {
        let c = Config::parse_from(["helm-server"]);
        assert_eq!(c.offline_alert_mins, 30);
        assert_eq!(c.log_dir, "./logs");
        assert_eq!(c.mcp_tier, 2);
    }

    /// A1：出厂默认值必须被识别为「弱」。
    #[test]
    fn insecure_defaults_detects_factory_values() {
        let c = Config::parse_from(["helm-server"]);
        let weak = c.insecure_defaults();
        assert!(weak.contains(&"HELM_SERVER_TOKEN"));
        assert!(weak.contains(&"HELM_JWT_SECRET"));
    }

    /// A1：显式配置强值后不再报弱。
    #[test]
    fn insecure_defaults_clean_after_hardening() {
        let c = Config::parse_from([
            "helm-server",
            "--server-token",
            "9f2c1e7a5b8d4f60",
            "--jwt-secret",
            "b71d0c3a9e548f26",
        ]);
        assert!(c.insecure_defaults().is_empty());
    }

    /// A1：HELM_REQUIRE_STRONG_DEFAULTS=1 时弱值必须拒绝启动。
    #[test]
    fn require_strong_defaults_refuses_weak() {
        let c = Config::parse_from(["helm-server", "--require-strong-defaults"]);
        assert!(c.guard_insecure_defaults().is_err());
    }

    /// A2：轮换 token 集合 = 主 token + 逗号分隔的额外 token（去重去空）。
    #[test]
    fn accepted_tokens_unions_and_dedups() {
        let c = Config::parse_from([
            "helm-server",
            "--server-token",
            "old-token",
            "--server-tokens",
            " new-token , old-token ,, ",
        ]);
        assert_eq!(c.accepted_server_tokens(), vec!["old-token", "new-token"]);
    }

    /// A2：空 token 不进入接受集合（fail-closed 的输入侧保证）。
    #[test]
    fn accepted_tokens_skips_empty() {
        let c = Config::parse_from([
            "helm-server",
            "--server-token",
            "",
            "--server-tokens",
            " , ",
        ]);
        assert!(c.accepted_server_tokens().is_empty());
    }
}
