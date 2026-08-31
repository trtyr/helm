//! 应用层：认证（登录、JWT 签发与校验、seed 管理员）。

use crate::store::{Db, user_repo::UserRepo};
use anyhow::Result;
use jsonwebtoken::{EncodingKey, Header, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// username
    pub sub: String,
    pub role: String,
    pub exp: usize,
}

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
        let data = decode::<Claims>(
            token,
            &jsonwebtoken::DecodingKey::from_secret(self.jwt_secret.as_bytes()),
            &jsonwebtoken::Validation::default(),
        )?;
        Ok(data.claims)
    }

    /// 若 users 表为空，seed 默认管理员 admin / admin123。
    pub async fn seed_admin(&self) -> Result<()> {
        let repo = UserRepo::new(self.db.clone());
        if repo.count().await? == 0 {
            let hash = bcrypt::hash("admin123", bcrypt::DEFAULT_COST)?;
            repo.create("admin", &hash, "admin").await?;
            tracing::info!("seeded default admin user 'admin'");
        }
        Ok(())
    }

    fn issue_token(&self, username: &str, role: &str) -> Result<String> {
        let exp = chrono::Utc::now().timestamp() as usize + 24 * 3600;
        let claims = Claims {
            sub: username.to_string(),
            role: role.to_string(),
            exp,
        };
        let token = encode(
            &Header::default(),
            &claims,
            &EncodingKey::from_secret(self.jwt_secret.as_bytes()),
        )?;
        Ok(token)
    }
}
