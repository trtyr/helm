//! 应用层：Agent 生命周期（列出 / 下线注销 / 卸载）。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::store::Db;
use crate::store::agent_repo::{AgentRepo, AgentRow, HostOsDetails};
use crate::store::host_repo::HostRepo;
use helm_proto::pb::{Register, SelfDestruct, ServerMessage, server_message};
use uuid::Uuid;

/// Agent 生命周期用例：下线 / 注销 / 卸载编排。
#[derive(Clone)]
pub struct AgentLifecycleService {
    db: Db,
    registry: ConnectionRegistry,
}

impl AgentLifecycleService {
    pub fn new(db: Db, registry: ConnectionRegistry) -> Self {
        Self { db, registry }
    }

    /// 列出所有 agent（附在线状态）。
    pub async fn list(&self) -> Result<Vec<AgentView>> {
        let agents = AgentRepo::new(self.db.clone()).list_all().await?;
        let mut views = Vec::with_capacity(agents.len());
        for agent in agents {
            let online = self.registry.is_online(&agent.id).await;
            views.push(AgentView { agent, online });
        }
        Ok(views)
    }

    /// 下线/注销：删除 agent 记录 + 孤儿 host 软删除，并解除连接。
    /// agent 离线时无法送达指令：删记录 + 标记挂起，重连瞬间补执行注销。
    pub async fn deregister(&self, agent_id: &str) -> Result<()> {
        let agent_repo = AgentRepo::new(self.db.clone());
        // 存在性校验必须先于挂起标记（EN-3）：对不存在的 agent 返回 404 时不得
        // 留下 pending_offline 副作用——否则同名 id 的 Agent 下次上线瞬间即被自毁。
        let host_id = agent_repo
            .get_host_id(agent_id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("agent: {agent_id}")))?;
        if !self.registry.is_online(agent_id).await {
            agent_repo
                .mark_pending_offline(agent_id, "deregister")
                .await?;
        }

        agent_repo.delete(agent_id).await?;

        // 若该 host 已无其他 agent，软删除 host。
        let remaining = agent_repo.list_agent_ids(host_id).await?;
        if remaining.is_empty() {
            HostRepo::new(self.db.clone()).soft_delete(host_id).await?;
        }

        // 注销连接并踢其入站任务退出，旧 gRPC 流与 TCP 连接才能释放
        if let Some(old) = self.registry.unregister(agent_id).await {
            old.kick();
        }
        Ok(())
    }

    /// 卸载：下发 SelfDestruct 指令，随后注销 DB 记录。
    /// 返回是否即时送达；agent 离线时标记挂起（false），重连瞬间自动补下线。
    pub async fn uninstall(&self, agent_id: &str, remove_binary: bool) -> Result<bool> {
        if !self.registry.is_online(agent_id).await {
            AgentRepo::new(self.db.clone())
                .mark_pending_offline(agent_id, "uninstall")
                .await?;
            return Ok(false);
        }
        let msg = ServerMessage {
            kind: Some(server_message::Kind::SelfDestruct(SelfDestruct {
                remove_binary,
            })),
        };
        self.registry
            .send(agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;

        self.deregister(agent_id).await?;
        Ok(true)
    }

    // ---- G3 收口（2026-09-21）：grpc 层不再直构 store 仓储 ----
    // 注册 / 心跳 / 挂起动作这三类「发生在 gRPC 连接生命周期内、但语义属于业务用例」的
    // 写操作，原先由 grpc 层直接 `AgentRepo::new(db)` 落库，绕过了应用层。
    // 收口到本服务后，分层方向（grpc → application → store）重新成立。

    /// 心跳：刷新 `agents.last_heartbeat`。
    pub async fn update_heartbeat(&self, agent_id: &str, ts_ms: u64) -> Result<()> {
        AgentRepo::new(self.db.clone())
            .update_heartbeat(agent_id, ts_ms)
            .await?;
        Ok(())
    }

    /// 首次注册落库（reverse 模式）：新建或更新 host + agents 行，返回 host_id。
    pub async fn register_agent(
        &self,
        agent_id: &str,
        register: &Register,
        public_ip: &str,
    ) -> Result<Uuid> {
        let id = AgentRepo::new(self.db.clone())
            .register(
                agent_id,
                &register.version,
                host_field(register, |h| h.hostname.as_str()),
                host_field(register, |h| h.os.as_str()),
                host_field(register, |h| h.arch.as_str()),
                host_field(register, |h| h.platform.as_str()),
                public_ip,
                &register
                    .host
                    .as_ref()
                    .map(|h| h.local_ips.clone())
                    .unwrap_or_default(),
                register.host.as_ref().map(|h| h.elevated).unwrap_or(false),
                os_details(register),
            )
            .await?;
        Ok(id)
    }

    /// forward 模式落库：挂到 `host_id` 声明的 host 下（不新建 host）。
    pub async fn register_under_host(
        &self,
        host_id: Uuid,
        agent_id: &str,
        register: &Register,
        public_ip: &str,
    ) -> Result<()> {
        AgentRepo::new(self.db.clone())
            .register_under_host(
                host_id,
                agent_id,
                &register.version,
                host_field(register, |h| h.os.as_str()),
                host_field(register, |h| h.arch.as_str()),
                host_field(register, |h| h.platform.as_str()),
                public_ip,
                &register
                    .host
                    .as_ref()
                    .map(|h| h.local_ips.clone())
                    .unwrap_or_default(),
                register.host.as_ref().map(|h| h.elevated).unwrap_or(false),
                os_details(register),
            )
            .await?;
        Ok(())
    }

    /// 取出掉线期间挂起的动作（`deregister` / `uninstall`），reconnect 时补执行。
    pub async fn take_pending_offline(&self, agent_id: &str) -> Result<Option<String>> {
        let pending = AgentRepo::new(self.db.clone())
            .take_pending_offline(agent_id)
            .await?;
        Ok(pending)
    }

    /// 仅删 agents 行（延迟卸载路径；host 是否软删除由调用方决定）。
    pub async fn remove_record(&self, agent_id: &str) -> Result<()> {
        AgentRepo::new(self.db.clone()).delete(agent_id).await?;
        Ok(())
    }
}

/// 取 `Register.host` 的某个字符串字段，缺失时按空串（与调用点原语义一致）。
fn host_field<'a>(
    register: &'a Register,
    pick: impl Fn(&'a helm_proto::pb::HostInfo) -> &'a str,
) -> &'a str {
    register.host.as_ref().map(pick).unwrap_or("")
}

/// `Register.host` → 落库用的 OS 明细（缺失时全空）。
fn os_details(register: &Register) -> HostOsDetails<'_> {
    register
        .host
        .as_ref()
        .map(|h| HostOsDetails {
            os_version: &h.os_version,
            kernel: &h.kernel,
            uptime_secs: h.uptime_secs,
        })
        .unwrap_or_default()
}

/// Agent 列表视图：附在线状态。
#[derive(Debug, serde::Serialize)]
pub struct AgentView {
    #[serde(flatten)]
    agent: AgentRow,
    online: bool,
}
