//! 应用层：审计日志（记录关键操作 + 查询）。

use crate::domain::Result;
use crate::store::{Db, audit_repo::AuditRepo};
use serde_json::Value;

/// 审计用例。
#[derive(Clone)]
pub struct AuditService {
    db: Db,
}

impl AuditService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 记录一条审计（actor/action/resource/detail）。
    pub async fn record(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        detail: Value,
    ) -> Result<()> {
        AuditRepo::new(self.db.clone())
            .insert(actor, action, resource, &detail)
            .await?;
        Ok(())
    }

    /// 记录一条审计；失败只告警，不打断主流程。
    ///
    /// **为什么用这个而不是 `let _ = record(...)`**（T3 错误契约）：审计写入不参与业务
    /// 事务（HTTP handler 不该因审计失败而回滚已经发生的事实），但**静默丢审计等于丢
    /// 追责链**——历史写法 `let _ = AuditService::new(db).record(...)` 在写库失败时不留
    /// 任何痕迹。本方法把失败提升为 `warn` 级日志（带 actor/action/resource 与错误），
    /// 使「审计写不进去」在 server 日志里可见。
    pub async fn record_best_effort(
        &self,
        actor: &str,
        action: &str,
        resource: &str,
        detail: Value,
    ) {
        if let Err(e) = self.record(actor, action, resource, detail).await {
            tracing::warn!(
                actor,
                action,
                resource,
                error = %e,
                "audit record failed (best-effort: business result unaffected)"
            );
        }
    }

    /// 列出最近审计记录。
    pub async fn list(&self, limit: i64) -> Result<Vec<crate::store::audit_repo::AuditRow>> {
        Ok(AuditRepo::new(self.db.clone()).list(limit).await?)
    }

    /// 分页列出审计记录。sort（P001-T1c）透传仓储层；q/action（P001-T1）搜索与动作过滤。
    #[allow(clippy::too_many_arguments)]
    pub async fn list_paged(
        &self,
        limit: i64,
        offset: i64,
        sort: Option<(String, bool)>,
        q: Option<&str>,
        action: Option<&str>,
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<crate::store::audit_repo::AuditRow>> {
        Ok(AuditRepo::new(self.db.clone())
            .list_paged(limit, offset, sort, q, action, from, to)
            .await?)
    }
}
