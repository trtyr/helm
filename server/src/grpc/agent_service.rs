//! AgentService 实现：处理 Agent 反向连入的双向流。
//!
//! 结构（G1/G2，2026-09-20）：`open_channel` 只做阶段编排；「握手 → 落库 → 登记
//! 连接 → 回执 → 补执行 → 上线事件 → 入站泵」各自是独立可读的方法；构造依赖经
//! [`AgentServiceDeps`] 一次性传入。

use std::pin::Pin;

use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use helm_proto::pb::{
    AgentMessage, Register, RegisterAck, ServerMessage, agent_message,
    agent_service_server::AgentService, server_message,
};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::codegen::async_trait;
use tonic::{Request, Response, Status, Streaming};

mod registration;

/// 构造 `AgentServiceImpl` 所需的全部依赖（G2：收口参数爆炸）。
pub struct AgentServiceDeps {
    pub registry: ConnectionRegistry,
    pub transfers: TransferRegistry,
    pub sessions: SessionRegistry,
    pub file_list: FileListRegistry,
    pub query: QueryRegistry,
    pub streams: StreamRegistry,
    /// 指标落库队列（E2）。
    pub metrics: crate::application::metric_sink::MetricSink,
    pub db: Db,
    /// 可接受的 Agent token 全集（A2：主 token + 轮换中的额外 token）
    pub server_tokens: Vec<String>,
}

/// AgentService 实现：持有连接注册表、数据库与认证 token。
pub struct AgentServiceImpl {
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
    sessions: SessionRegistry,
    file_list: FileListRegistry,
    query: QueryRegistry,
    streams: StreamRegistry,
    metrics: crate::application::metric_sink::MetricSink,
    db: Db,
    /// 可接受的 Agent token 全集（A2：轮换期新旧并存）
    server_tokens: Vec<String>,
}

impl AgentServiceImpl {
    pub fn new(deps: AgentServiceDeps) -> Self {
        Self {
            registry: deps.registry,
            transfers: deps.transfers,
            sessions: deps.sessions,
            file_list: deps.file_list,
            query: deps.query,
            streams: deps.streams,
            metrics: deps.metrics,
            db: deps.db,
            server_tokens: deps.server_tokens,
        }
    }

    /// 校验注册 token：命中等值集合中任意一个即通过（A2：恒定时间 + 多 token 轮换）。
    fn verify_token(&self, token: &str) -> bool {
        token_matches_any(&self.server_tokens, token)
    }
}

/// 读取首帧并解析为 `Register`（首条消息必须是 Register）。
async fn read_register(inbound: &mut Streaming<AgentMessage>) -> Result<Register, Status> {
    let first = inbound
        .message()
        .await?
        .ok_or_else(|| Status::invalid_argument("channel closed before Register"))?;
    match first.kind {
        Some(agent_message::Kind::Register(r)) => Ok(r),
        _ => Err(Status::invalid_argument("first message must be Register")),
    }
}

/// 连接源地址（NAT 后即出口公网地址）；`into_inner` 会拿走 request，须先取。
fn peer_ip(request: &Request<Streaming<AgentMessage>>) -> String {
    request
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default()
}

/// 回 RegisterAck。
///
/// 发送失败即对端已断：本次入站会话随即由泵循环检出并注销，无需在此重复告警。
async fn send_register_ack(tx: &mpsc::Sender<ServerMessage>) {
    let ack = ServerMessage {
        kind: Some(server_message::Kind::RegisterAck(RegisterAck {
            ok: true,
            message: "registered".into(),
            heartbeat_interval_secs: 10,
        })),
    };
    let _ = tx.send(ack).await; // 对端已断：ack 无处送达，泵循环随即检出并注销（见本函数文档）
}

#[async_trait]
impl AgentService for AgentServiceImpl {
    type OpenChannelStream =
        Pin<Box<dyn tokio_stream::Stream<Item = Result<ServerMessage, Status>> + Send>>;

    /// 反向连接建立：按阶段编排（握手 → 落库 → 登记 → 回执 → 补执行 → 上线事件 → 入站泵）。
    async fn open_channel(
        &self,
        request: Request<Streaming<AgentMessage>>,
    ) -> Result<Response<Self::OpenChannelStream>, Status> {
        // 阶段 1 · 握手：首帧必须是 Register，token 严格匹配
        let public_ip = peer_ip(&request);
        let mut inbound = request.into_inner();
        let register = read_register(&mut inbound).await?;
        if !self.verify_token(&register.token) {
            return Err(Status::unauthenticated("invalid token"));
        }
        let agent_id = register.agent_id.clone();

        // 阶段 2 · 落库 host + agent
        let host_id = self.persist_agent(&agent_id, &register, &public_ip).await;

        // 阶段 3 · 登记连接（容量满即拒绝；同 id 顶掉旧连接）
        let (tx, rx) = mpsc::channel::<ServerMessage>(64);
        let registration = self
            .register_connection(&agent_id, &register, tx.clone())
            .await?;

        // 阶段 4 · 回 RegisterAck
        send_register_ack(&tx).await;

        // 阶段 5 · 补执行掉线期间挂起的下线与取消
        let deferred_offline = self.run_deferred_offline(&agent_id, &tx).await;
        self.run_deferred_cancels(&agent_id, &tx).await;

        // 阶段 6 · 上线事件（补执行/重复注册时不通知）
        let replaced = registration.replaced.is_some();
        self.notify_online(host_id, &register, deferred_offline, replaced)
            .await;

        // 阶段 7 · 后台泵入站流（流结束时注销 + 落离线事件）
        let hostname = register
            .host
            .as_ref()
            .map(|h| h.hostname.clone())
            .unwrap_or_default();
        self.spawn_inbound_pump(inbound, registration, host_id, hostname, agent_id);

        let outbound = ReceiverStream::new(rx).map(Ok);
        Ok(Response::new(Box::pin(outbound)))
    }
}

/// 校验注册 token：恒定时间比较，且 server_token 为空时拒绝所有（纯函数，便于测试）。
///
/// **A2 修复（2026-09-20）**：原实现是裸 `provided == server_token`（短路比较，逐字节提前返回），
/// 理论上可被计时侧信道逐步猜出 token 前缀。现改为**恒定时间**比较。
///
/// 注意：这里手写恒定时间比较而不引入 `subtle`——逻辑只有 6 行且可测，
/// 避免为一个比较引入新依赖（若将来需要抗编译器优化，替换为 `subtle::ConstantTimeEq` 即可）。
pub fn token_matches(server_token: &str, provided: &str) -> bool {
    !server_token.is_empty() && ct_eq(server_token.as_bytes(), provided.as_bytes())
}

/// **A2 轮换支持**：命中集合中任意一个 token 即通过（新旧 token 并存 → 不停机轮换）。
///
/// 比较仍是恒定时间的；**且刻意不做提前返回**——逐个比完再归并，避免用「哪个 token 先比较」
/// 泄漏在用的是哪一个。
pub fn token_matches_any(tokens: &[String], provided: &str) -> bool {
    if provided.is_empty() {
        return false;
    }
    let mut hit = false;
    for t in tokens {
        if !t.is_empty() {
            hit |= ct_eq(t.as_bytes(), provided.as_bytes());
        }
    }
    hit
}

/// 恒定时间字节比较：长度不同直接判否（长度本身不是秘密），长度相同则全量异或累积。
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 由执行结果判断 Job 终态（纯函数，便于测试）。
/// `cancelled`/`timed_out` 由 agent 显式上报（JobCancel 杀进程 / timeout_secs 超时），
/// 优先于 error/exit_code 判定（EN-64）。
pub fn job_status(
    error: Option<&str>,
    exit_code: Option<i32>,
    cancelled: bool,
    timed_out: bool,
) -> &'static str {
    if cancelled {
        "cancelled"
    } else if timed_out {
        "timed_out"
    } else if error.is_some() {
        "failed"
    } else {
        match exit_code {
            Some(0) => "succeeded",
            Some(_) => "failed",
            None => "succeeded",
        }
    }
}

/// 将 agent 上报的服务状态映射为 DB status（纯函数，便于测试）。
pub fn map_service_status(status: &str) -> Option<&'static str> {
    match status {
        "running" => Some("running"),
        "failed" => Some("failed"),
        "exited" => Some("stopped"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_strict() {
        assert!(token_matches("secret", "secret"));
        assert!(!token_matches("secret", "wrong"));
        assert!(!token_matches("secret", ""));
        // 空 server_token 也拒绝（默认值非空，严格匹配）
        assert!(!token_matches("", ""));
        // 长度不同、同前缀必须判否（恒定时间比较的边界）
        assert!(!token_matches("secret-long", "secret"));
        assert!(!token_matches("secret", "secret-long"));
    }

    /// A2：轮换集合——任一新旧 token 均可注册，未知 token 拒绝，空 token 拒绝。
    #[test]
    fn token_matches_any_supports_rotation() {
        let tokens = vec!["old-token".to_string(), "new-token".to_string()];
        assert!(
            token_matches_any(&tokens, "old-token"),
            "未退场的旧 token 仍可注册"
        );
        assert!(token_matches_any(&tokens, "new-token"), "新 token 可注册");
        assert!(!token_matches_any(&tokens, "other"), "未知 token 拒绝");
        assert!(!token_matches_any(&tokens, ""), "空 token 拒绝");
        // 集合里混入空串（配置里写了逗号）不得变成「空 token 放行」
        let with_empty = vec![String::new(), "real".to_string()];
        assert!(!token_matches_any(&with_empty, ""));
        assert!(token_matches_any(&with_empty, "real"));
    }

    /// A2：空集合 fail-closed（一个 token 都没配 → 拒绝一切注册）。
    #[test]
    fn token_matches_any_fails_closed_on_empty_set() {
        assert!(!token_matches_any(&[], "anything"));
    }

    #[test]
    fn job_status_rules() {
        assert_eq!(job_status(None, Some(0), false, false), "succeeded");
        assert_eq!(job_status(None, Some(1), false, false), "failed");
        assert_eq!(job_status(Some("boom"), None, false, false), "failed");
        assert_eq!(job_status(None, None, false, false), "succeeded");
        // EN-64：取消与超时标志优先于 error/exit_code
        assert_eq!(
            job_status(Some("killed"), Some(-9), true, false),
            "cancelled"
        );
        assert_eq!(job_status(Some("timeout"), None, false, true), "timed_out");
    }

    #[test]
    fn map_service_status_rules() {
        assert_eq!(map_service_status("running"), Some("running"));
        assert_eq!(map_service_status("failed"), Some("failed"));
        assert_eq!(map_service_status("exited"), Some("stopped"));
        assert_eq!(map_service_status("unknown"), None);
    }
}
