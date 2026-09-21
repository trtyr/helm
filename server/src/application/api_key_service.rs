//! 应用层：API key（机器对机器认证，决策 010）。
//!
//! 形态：`helm_` + 40 位 hex（20 随机字节）。库中仅存 sha256 哈希，
//! 明文只在创建响应里出现一次；吊销/过期在 SQL 与本层双重把关。

use crate::application::auth_service::Claims;
use crate::domain::Result;
use crate::store::Db;
use crate::store::api_key_repo::{ApiKeyRepo, ApiKeyRow};
use chrono::{DateTime, Utc};
use rand::RngCore;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// API key 明文前缀（认证分流依据：Bearer 值以它开头即走 API key 路径）。
pub const RAW_PREFIX: &str = "helm_";

/// API key 合成 Claims 的 role 标记（api-keys 管理端点据此拒绝 API key 自管）。
pub const ROLE_API_KEY: &str = "api-key";

/// API key 用例。
#[derive(Clone)]
pub struct ApiKeyService {
    db: Db,
}

impl ApiKeyService {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 创建 key：生成明文 + 落库哈希，返回（行, 明文）。明文仅此一次可见。
    /// scopes 为空 = 全功能；含未知 scope 拒绝创建。
    pub async fn create(
        &self,
        name: &str,
        expires_at: Option<DateTime<Utc>>,
        scopes: Vec<String>,
    ) -> Result<(ApiKeyRow, String)> {
        let bad = crate::application::scopes::invalid_ones(&scopes);
        if !bad.is_empty() {
            return Err(crate::domain::Error::InvalidArgument(format!(
                "unknown scopes: {}",
                bad.join(", ")
            )));
        }
        let raw = generate_raw_key();
        let row = ApiKeyRepo::new(self.db.clone())
            .insert(
                name,
                &hash_key(&raw),
                &display_prefix(&raw),
                expires_at,
                &scopes,
            )
            .await?;
        Ok((row, raw))
    }

    /// 校验明文 key：有效（存在 + 未吊销 + 未过期）则返回（key 名, scopes），并尽力刷新 last_used_at。
    pub async fn verify(&self, raw: &str) -> Result<Option<(String, Vec<String>)>> {
        let repo = ApiKeyRepo::new(self.db.clone());
        let Some(row) = repo.find_valid_by_hash(&hash_key(raw)).await? else {
            return Ok(None);
        };
        // 尽力刷新 last_used_at（见本方法文档：「尽力」）：失败只影响使用统计，
        // 不影响鉴权结果，故不计入错误路径。
        let _ = repo.touch_last_used(row.id).await;
        Ok(Some((row.name, row.scopes)))
    }

    /// 按 id 查（含已吊销，管理端点用）。
    pub async fn get(&self, id: Uuid) -> Result<Option<ApiKeyRow>> {
        Ok(ApiKeyRepo::new(self.db.clone()).get(id).await?)
    }

    /// 分页列出（按创建时间倒序）。
    pub async fn list_paged(&self, limit: i64, offset: i64) -> Result<Vec<ApiKeyRow>> {
        Ok(ApiKeyRepo::new(self.db.clone())
            .list_paged(limit, offset)
            .await?)
    }

    /// 吊销（幂等）。
    pub async fn revoke(&self, id: Uuid) -> Result<()> {
        ApiKeyRepo::new(self.db.clone()).revoke(id).await?;
        Ok(())
    }
}

/// 生成明文 key：`helm_` + 20 随机字节的 hex（40 字符）。
pub fn generate_raw_key() -> String {
    let mut bytes = [0u8; 20];
    rand::rng().fill_bytes(&mut bytes);
    format!("{RAW_PREFIX}{}", hex::encode(bytes))
}

/// 明文 → sha256 hex（库中存储形态）。
pub fn hash_key(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}

/// 明文 → 展示前缀（列表中可识别但不泄漏完整 key）。
pub fn display_prefix(raw: &str) -> String {
    raw.chars().take(12).collect()
}

/// API key 命中后合成的 Claims：`sub` 作为审计 actor（`api-key:<name>`），
/// `role` 标记来源，api-keys 管理端点据此只放行 JWT；scopes 供路由级授权。
pub fn claims_for(name: &str, scopes: Vec<String>) -> Claims {
    Claims {
        sub: format!("api-key:{name}"),
        role: ROLE_API_KEY.to_string(),
        exp: chrono::Utc::now().timestamp() as usize,
        scopes,
        // A5：API key 没有「用户 token 版本」概念——该字段只对 JWT 分支有意义
        // （中间件 verify_revocable 只在校验 JWT 时查库核对），此处填默认值。
        tv: 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_key_format() {
        let raw = generate_raw_key();
        assert!(raw.starts_with("helm_"), "raw: {raw}");
        assert_eq!(raw.len(), RAW_PREFIX.len() + 40);
        assert!(
            raw[RAW_PREFIX.len()..]
                .chars()
                .all(|c| c.is_ascii_hexdigit()),
            "body should be hex: {raw}"
        );
    }

    #[test]
    fn raw_keys_are_unique() {
        let a = generate_raw_key();
        let b = generate_raw_key();
        assert_ne!(a, b);
    }

    #[test]
    fn hash_is_stable_and_hex() {
        let h1 = hash_key("helm_abc123");
        let h2 = hash_key("helm_abc123");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
        assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(h1, hash_key("helm_other"));
    }

    #[test]
    fn display_prefix_keeps_head() {
        let raw = "helm_0123456789abcdef";
        assert_eq!(display_prefix(raw), "helm_0123456");
    }

    #[test]
    fn claims_mark_api_key_role() {
        let claims = claims_for("ci-bot", vec!["exec".into()]);
        assert_eq!(claims.sub, "api-key:ci-bot");
        assert_eq!(claims.role, ROLE_API_KEY);
        assert!(claims.has_scope("exec"));
        assert!(!claims.has_scope("files"));
    }

    #[test]
    fn empty_scopes_mean_full_access() {
        let claims = claims_for("legacy", Vec::new());
        assert!(claims.has_scope("files"));
        assert!(claims.has_scope("ir"));
    }
}
