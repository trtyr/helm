use clap::Parser;

/// 出厂初始管理员口令（公开值，与 `helm_bootstrap_admin_password` 的 default_value 一致，
/// 有单测 `bootstrap_admin_default_matches_const` 钉住两处不漂移）。
///
/// **刻意留在 config 层做「是否出厂值」的判定**：出厂口令危不危险取决于**库是否为空**，
/// 所以硬失败放在真正要拿它建账号的那一刻（`seed_admin`），而不是启动期的配置检查——
/// 否则既有部署（库非空、该配置根本不会被用到）升个级就会被拦住启动。
pub const FACTORY_ADMIN_PASSWORD: &str = "admin123";

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

    /// 拒绝以弱默认凭据启动（A1）：置真时若仍在使用默认 token/密钥则**启动失败**；
    /// 默认只打 ERROR 日志（开发/CI 与 e2e 脚本依赖默认值，硬失败会把它们一起打断）。
    ///
    /// 取值（Q006 起）：**推荐 `true`**，同时兼容 `1/0/yes/no`；CLI 与环境变量同一套解析
    /// （`--require-strong-defaults` 裸 flag 等价于 `=true`）。
    #[arg(
        long,
        env = "HELM_REQUIRE_STRONG_DEFAULTS",
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub require_strong_defaults: bool,

    /// JWT 签名密钥（生产必须配置强随机值）
    #[arg(long, env = "HELM_JWT_SECRET", default_value = "dev-secret-change-me")]
    pub jwt_secret: String,

    /// 初始管理员用户名（**仅当 users 表为空**、首次启动建号时生效；之后改名走控制台/API）。
    #[arg(long, env = "HELM_BOOTSTRAP_ADMIN_USER", default_value = "admin")]
    pub bootstrap_admin_user: String,

    /// 初始管理员口令（**仅当 users 表为空**、首次启动建号时生效）。
    ///
    /// 默认值是公开的出厂值 [`FACTORY_ADMIN_PASSWORD`]：真正要拿它建账号时，
    /// `HELM_REQUIRE_STRONG_DEFAULTS=true` 下**拒绝启动**，否则打 ERROR 日志。
    #[arg(
        long,
        env = "HELM_BOOTSTRAP_ADMIN_PASSWORD",
        default_value = "admin123"
    )]
    pub bootstrap_admin_password: String,

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

    /// 数据保留天数（C1 + T008，默认 90）：超过的 jobs / audit_logs / file_transfers、
    /// 时序类（metrics/alerts/notifications）与 status_events 由后台清理；
    /// IR 取证表有独立策略（见下两项配置）
    #[arg(long, env = "HELM_RETENTION_DAYS", default_value = "90")]
    pub retention_days: i64,

    /// IR 快照每主机保留条数（T009，默认 20）：快照是取证资产（基线对比/差异取证），
    /// 按「每主机条数」封顶而非按时间一刀切；0 = 不清理
    #[arg(long, env = "HELM_IR_SNAPSHOT_KEEP_PER_AGENT", default_value = "20")]
    pub ir_snapshot_keep_per_agent: i64,

    /// IR 页面缓存保留天数（T009，默认 30）：超过该天数未被扫描刷新的缓存行被清理
    /// （治陈旧内容与已注销主机留下的死缓存）；0 = 不清理
    #[arg(long, env = "HELM_IR_PAGE_CACHE_TTL_DAYS", default_value = "30")]
    pub ir_page_cache_ttl_days: i64,

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

    /// 是否启用 mTLS（Agent↔Server 双向认证）。取值（Q006）：**推荐 `true`**，
    /// 兼容 `1/0/yes/no`；裸 `--mtls` 等价于 `=true`。
    #[arg(
        long,
        env = "HELM_MTLS",
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
    pub mtls: bool,

    /// TLS 材料目录（CA/server 证书持久化；mTLS 部署强烈建议，保证重启后 CA 稳定）
    #[arg(long, env = "HELM_TLS_DIR", default_value = "")]
    pub tls_dir: String,

    /// 离线签发 agent 证书三件套后退出（forward 预置分发用，不启动服务）。
    /// 取值（Q006）：**推荐 `true`**，兼容 `1/0/yes/no`；裸 `--issue-cert` 等价于 `=true`。
    #[arg(
        long,
        env = "HELM_ISSUE_CERT",
        num_args = 0..=1,
        default_value = "false",
        default_missing_value = "true",
        value_parser = clap::builder::BoolishValueParser::new()
    )]
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
    ///
    /// 注意：初始管理员口令（`HELM_BOOTSTRAP_ADMIN_PASSWORD`）**刻意不在本清单里**——
    /// 它只在库为空时被用到，是否危险取决于库状态，故判定放在 `seed_admin` 那一刻
    /// （见 [`FACTORY_ADMIN_PASSWORD`] 的说明）。
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

    /// 初始管理员口令是否仍是出厂值（供 `seed_admin` 在建号那一刻做策略判定）。
    pub fn bootstrap_admin_password_is_factory(&self) -> bool {
        self.bootstrap_admin_password.trim() == FACTORY_ADMIN_PASSWORD
    }

    /// A1：启动期弱值守卫——命中即 ERROR；`HELM_REQUIRE_STRONG_DEFAULTS=true` 时拒绝启动。
    /// （Q006 起改用 `BoolishValueParser`：**推荐 true**，同时兼容 `1/0/yes/no`。）
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
                "HELM_REQUIRE_STRONG_DEFAULTS=true 但仍在用弱默认凭据: {}",
                weak.join(", ")
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{Config, FACTORY_ADMIN_PASSWORD};
    use clap::Parser;

    /// P003 T2/T3 契约：默认值与文档口径一致（HELM_OFFLINE_ALERT_MINS=30、HELM_LOG_DIR=./logs）。
    #[test]
    fn defaults_match_docs() {
        let c = Config::parse_from(["helm-server"]);
        assert_eq!(c.offline_alert_mins, 30);
        assert_eq!(c.log_dir, "./logs");
        assert_eq!(c.mcp_tier, 2);
    }

    /// 初始管理员：出厂默认值、以及「常量与 clap default_value 不漂移」的钉子。
    #[test]
    fn bootstrap_admin_default_matches_const() {
        let c = Config::parse_from(["helm-server"]);
        assert_eq!(c.bootstrap_admin_user, "admin");
        assert_eq!(c.bootstrap_admin_password, FACTORY_ADMIN_PASSWORD);
        assert!(c.bootstrap_admin_password_is_factory());
    }

    /// 显式配置了非出厂口令后，不再判定为出厂值（→ 不会触发拒绝启动）。
    #[test]
    fn bootstrap_admin_custom_not_factory() {
        let c = Config::parse_from([
            "helm-server",
            "--bootstrap-admin-user",
            "trtyr",
            "--bootstrap-admin-password",
            "not-the-factory-one",
        ]);
        assert_eq!(c.bootstrap_admin_user, "trtyr");
        assert!(!c.bootstrap_admin_password_is_factory());
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

    /// A1：HELM_REQUIRE_STRONG_DEFAULTS=true 时弱值必须拒绝启动。
    #[test]
    fn require_strong_defaults_refuses_weak() {
        let c = Config::parse_from(["helm-server", "--require-strong-defaults"]);
        assert!(c.guard_insecure_defaults().is_err());
    }

    /// Q006：布尔开关吃多种写法（CLI 与环境变量共用同一 `value_parser`）。
    #[test]
    fn bool_flag_accepts_truthy_and_falsey_forms() {
        for v in ["true", "1", "yes", "YES", "on"] {
            let c = Config::parse_from(["helm-server", &format!("--mtls={v}")]);
            assert!(c.mtls, "--mtls={v} 应解析为真");
        }
        for v in ["false", "0", "no", "NO", "off"] {
            let c = Config::parse_from(["helm-server", &format!("--mtls={v}")]);
            assert!(!c.mtls, "--mtls={v} 应解析为假");
        }
        // 裸 flag（`default_missing_value`）= true；缺省 = false
        assert!(Config::parse_from(["helm-server", "--mtls"]).mtls);
        assert!(!Config::parse_from(["helm-server"]).mtls);
    }

    /// Q006：三个布尔开关必须同一套行为（防止只改了一个）。
    #[test]
    fn all_bool_flags_share_the_same_parser() {
        let with_values = Config::parse_from([
            "helm-server",
            "--mtls=1",
            "--issue-cert=yes",
            "--require-strong-defaults=0",
        ]);
        assert!(with_values.mtls);
        assert!(with_values.issue_cert);
        assert!(!with_values.require_strong_defaults);

        let bare = Config::parse_from([
            "helm-server",
            "--mtls",
            "--issue-cert",
            "--require-strong-defaults",
        ]);
        assert!(bare.mtls && bare.issue_cert && bare.require_strong_defaults);
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
