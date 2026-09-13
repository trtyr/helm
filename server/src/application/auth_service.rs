//! 应用层：认证（登录、JWT 签发与校验、seed 管理员、单用户账号管理）。

use crate::domain::{Error, Result};
use crate::store::{Db, user_repo::UserRepo};
use jsonwebtoken::{EncodingKey, Header, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// username
    pub sub: String,
    pub role: String,
    pub exp: usize,
    /// API key 的 scope 列表；空 = 全功能（JWT 即此形态，serde default 兼容旧 token）。
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl Claims {
    /// 是否持有某 scope：空列表 = 不受限（存量 key 与 JWT 均为此形态）。
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes.is_empty() || self.scopes.iter().any(|s| s == scope)
    }
}

/// 账号视图（/auth/me；不含密码哈希）。
#[derive(Debug, Serialize)]
pub struct AccountView {
    pub username: String,
    pub role: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// 新密码最小长度。
pub const MIN_PASSWORD_LEN: usize = 6;

/// 认证用例。
#[derive(Clone)]
pub struct AuthService {
    db: Db,
    jwt_secret: String,
}

impl AuthService {
    pub fn new(db: Db, jwt_secret: String) -> Self {
        Self { db, jwt_secret }
    }

    /// 登录：校验密码，成功则签发 JWT。
    pub async fn login(&self, username: &str, password: &str) -> Result<Option<String>> {
        let user = match UserRepo::new(self.db.clone())
            .get_by_username(username)
            .await?
        {
            Some(u) => u,
            None => return Ok(None),
        };

        let ok = bcrypt::verify(password, &user.password_hash).unwrap_or(false);
        if !ok {
            return Ok(None);
        }

        Ok(Some(self.issue_token(&user.username, &user.role)?))
    }

    /// 校验 JWT，返回 claims。
    pub fn verify(&self, token: &str) -> Result<Claims> {
        verify_jwt(&self.jwt_secret, token)
    }

    /// 若 users 表为空，seed 默认管理员 admin / admin123。
    pub async fn seed_admin(&self) -> Result<()> {
        let repo = UserRepo::new(self.db.clone());
        if repo.count().await? == 0 {
            let hash = bcrypt::hash("admin123", bcrypt::DEFAULT_COST)
                .map_err(|e| Error::Internal(format!("bcrypt: {e}")))?;
            repo.create("admin", &hash, "admin").await?;
            tracing::info!("seeded default admin user 'admin'");
        }
        Ok(())
    }

    fn issue_token(&self, username: &str, role: &str) -> Result<String> {
        issue_jwt(&self.jwt_secret, username, role, 24 * 3600)
    }

    /// 当前账号（按 JWT sub 查库取权威数据；sub 已失效——如改名后旧 token——视为未授权）。
    pub async fn account(&self, username: &str) -> Result<AccountView> {
        let user = UserRepo::new(self.db.clone())
            .get_by_username(username)
            .await?
            .ok_or_else(|| Error::Unauthorized("user no longer exists; re-login".into()))?;
        Ok(AccountView {
            username: user.username,
            role: user.role,
            created_at: user.created_at,
        })
    }

    /// 修改密码：校验当前密码 → 更新哈希。已有 JWT 不失效（24h 自然过期）。
    pub async fn change_password(&self, username: &str, current: &str, new: &str) -> Result<()> {
        if new.len() < MIN_PASSWORD_LEN {
            return Err(Error::InvalidArgument(format!(
                "new password must be at least {MIN_PASSWORD_LEN} characters"
            )));
        }
        let repo = UserRepo::new(self.db.clone());
        let user = repo
            .get_by_username(username)
            .await?
            .ok_or_else(|| Error::Unauthorized("user no longer exists; re-login".into()))?;
        let ok = bcrypt::verify(current, &user.password_hash).unwrap_or(false);
        if !ok {
            return Err(Error::Unauthorized("current password incorrect".into()));
        }
        let hash = bcrypt::hash(new, bcrypt::DEFAULT_COST)
            .map_err(|e| Error::Internal(format!("bcrypt: {e}")))?;
        repo.update_password(user.id, &hash).await?;
        Ok(())
    }

    /// 修改用户名：校验当前密码 → 查重（UNIQUE，含软删行）→ 更新。
    /// 旧 JWT 的 sub 随即失效（/auth/me 等按 sub 查库的端点会要求重新登录）。
    pub async fn change_username(&self, username: &str, current: &str, new: &str) -> Result<()> {
        let new = new.trim();
        if new.is_empty() || new.len() > 64 {
            return Err(Error::InvalidArgument(
                "new username must be 1-64 characters".into(),
            ));
        }
        let repo = UserRepo::new(self.db.clone());
        let user = repo
            .get_by_username(username)
            .await?
            .ok_or_else(|| Error::Unauthorized("user no longer exists; re-login".into()))?;
        let ok = bcrypt::verify(current, &user.password_hash).unwrap_or(false);
        if !ok {
            return Err(Error::Unauthorized("current password incorrect".into()));
        }
        if repo.get_by_username(new).await?.is_some() {
            return Err(Error::InvalidArgument("username already taken".into()));
        }
        let updated = repo.update_username(user.id, new).await.map_err(|e| {
            // username 全表 UNIQUE（含软删行），预查重覆盖不到的冲突（23505）转 400
            if e.as_database_error().and_then(|d| d.code()).as_deref() == Some("23505") {
                Error::InvalidArgument("username already taken".into())
            } else {
                Error::from(e)
            }
        })?;
        if !updated {
            return Err(Error::Internal("user vanished during rename".into()));
        }
        Ok(())
    }
}

/// 签发 JWT（纯函数，便于测试）。
pub fn issue_jwt(secret: &str, username: &str, role: &str, ttl_secs: usize) -> Result<String> {
    let exp = chrono::Utc::now().timestamp() as usize + ttl_secs;
    let claims = Claims {
        scopes: Vec::new(),
        sub: username.to_string(),
        role: role.to_string(),
        exp,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| Error::Internal(format!("jwt encode: {e}")))?;
    Ok(token)
}

/// 校验 JWT（纯函数）。
pub fn verify_jwt(secret: &str, token: &str) -> Result<Claims> {
    let data = decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .map_err(|_| Error::Unauthorized("invalid token".into()))?;
    Ok(data.claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_roundtrip() {
        let token = issue_jwt("secret-key", "alice", "admin", 3600).unwrap();
        let claims = verify_jwt("secret-key", &token).unwrap();
        assert_eq!(claims.sub, "alice");
        assert_eq!(claims.role, "admin");
    }

    #[test]
    fn jwt_wrong_secret_rejected() {
        let token = issue_jwt("secret-a", "alice", "admin", 3600).unwrap();
        assert!(verify_jwt("secret-b", &token).is_err());
    }
}
