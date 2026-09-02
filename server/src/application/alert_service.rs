//! 应用层：告警（阈值判定 + 落库 + 查询）。

use crate::domain::Result;
use crate::store::Db;
use crate::store::alert_repo::{AlertRepo, AlertRow};
use uuid::Uuid;

/// 告警用例。
#[derive(Clone)]
pub struct AlertService {
    db: Db,
}

impl AlertService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 指标阈值（纯函数，便于测试）：超过即告警。
    pub fn threshold_for(name: &str) -> Option<f64> {
        match name {
            "cpu.usage" | "mem.percent" | "disk.usage" => Some(90.0),
            _ => None,
        }
    }

    /// 检查单条指标，超阈值则落 alerts 表。
    pub async fn check_and_record(&self, host_id: Uuid, name: &str, value: f64) -> Result<()> {
        let Some(threshold) = Self::threshold_for(name).filter(|t| value > *t) else {
            return Ok(());
        };
        AlertRepo::new(self.db.clone())
            .insert(host_id, name, threshold, value)
            .await?;
        Ok(())
    }

    /// 列出最近告警。
    pub async fn list(&self, limit: i64) -> Result<Vec<AlertRow>> {
        Ok(AlertRepo::new(self.db.clone()).list(limit).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn threshold_for_known_metrics() {
        assert_eq!(AlertService::threshold_for("cpu.usage"), Some(90.0));
        assert_eq!(AlertService::threshold_for("mem.percent"), Some(90.0));
        assert_eq!(AlertService::threshold_for("disk.usage"), Some(90.0));
        assert_eq!(AlertService::threshold_for("net.rx_bytes"), None);
        assert_eq!(AlertService::threshold_for("unknown"), None);
    }
}
