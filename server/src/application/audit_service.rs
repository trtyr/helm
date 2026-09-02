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

    /// 列出最近审计记录。
    pub async fn list(&self, limit: i64) -> Result<Vec<crate::store::audit_repo::AuditRow>> {
        Ok(AuditRepo::new(self.db.clone()).list(limit).await?)
    }

    /// 分页列出审计记录。
    pub async fn list_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<crate::store::audit_repo::AuditRow>> {
        Ok(AuditRepo::new(self.db.clone())
            .list_paged(limit, offset)
            .await?)
    }
}
