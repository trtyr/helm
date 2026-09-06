//! Host 仓储：hosts 表的读写。作为仓储层范式（后续 agent/job 等照此拆分）。

use crate::store::Db;
use sqlx::FromRow;
use uuid::Uuid;

/// `hosts` 表行。
#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct HostRow {
    pub id: Uuid,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub platform: String,
    pub tags: Vec<String>,
    pub conn_mode: String,
    pub addr: String,
    /// Server 看到的 Agent 连接源地址（外网视角）。
    pub public_ip: String,
    /// Agent 注册上报的本机网卡地址（内网视角，IPv4 在前）。
    pub local_ips: Vec<String>,
}

/// 新建主机参数。
#[derive(Debug, Clone)]
pub struct NewHost {
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub platform: String,
    pub tags: Vec<String>,
    pub conn_mode: String,
    pub addr: String,
}

/// hosts 表仓储。
#[derive(Clone)]
pub struct HostRepo {
    db: Db,
}

impl HostRepo {
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// 插入一台主机并返回完整行。
    pub async fn insert(&self, h: &NewHost) -> sqlx::Result<HostRow> {
        sqlx::query_as::<_, HostRow>(
            "INSERT INTO hosts (hostname, os, arch, platform, tags, conn_mode, addr)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips",
        )
        .bind(&h.hostname)
        .bind(&h.os)
        .bind(&h.arch)
        .bind(&h.platform)
        .bind(&h.tags)
        .bind(&h.conn_mode)
        .bind(&h.addr)
        .fetch_one(self.db.pool())
        .await
    }

    /// 按主机名查主机（未删除）。
    pub async fn get_by_hostname(&self, hostname: &str) -> sqlx::Result<Option<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "SELECT id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips
             FROM hosts WHERE hostname = $1 AND deleted_at IS NULL LIMIT 1",
        )
        .bind(hostname)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 列出所有未删除主机（按创建时间倒序）。
    pub async fn list(&self) -> sqlx::Result<Vec<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "SELECT id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips
             FROM hosts WHERE deleted_at IS NULL ORDER BY created_at DESC",
        )
        .fetch_all(self.db.pool())
        .await
    }

    /// 按 id 查主机（未删除）。
    pub async fn get(&self, id: Uuid) -> sqlx::Result<Option<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "SELECT id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips
             FROM hosts WHERE id = $1 AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 更新主机信息，返回更新后的行（未找到返回 None）。
    pub async fn update(&self, id: Uuid, h: &NewHost) -> sqlx::Result<Option<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "UPDATE hosts SET hostname = $2, os = $3, arch = $4, platform = $5,
                 tags = $6, conn_mode = $7, addr = $8, updated_at = now()
             WHERE id = $1 AND deleted_at IS NULL
             RETURNING id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips",
        )
        .bind(id)
        .bind(&h.hostname)
        .bind(&h.os)
        .bind(&h.arch)
        .bind(&h.platform)
        .bind(&h.tags)
        .bind(&h.conn_mode)
        .bind(&h.addr)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 分页列出（按创建时间倒序）。
    pub async fn list_paged(&self, limit: i64, offset: i64) -> sqlx::Result<Vec<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "SELECT id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips
             FROM hosts WHERE deleted_at IS NULL ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
    }

    /// 按标签过滤主机（`tag = ANY(tags)`）。
    pub async fn list_by_tag(&self, tag: &str) -> sqlx::Result<Vec<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "SELECT id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips
             FROM hosts WHERE deleted_at IS NULL AND $1 = ANY(tags) ORDER BY created_at DESC",
        )
        .bind(tag)
        .fetch_all(self.db.pool())
        .await
    }

    /// 覆盖设置主机标签，返回更新后的行（未找到返回 None）。
    pub async fn set_tags(&self, id: Uuid, tags: &[String]) -> sqlx::Result<Option<HostRow>> {
        sqlx::query_as::<_, HostRow>(
            "UPDATE hosts SET tags = $2, updated_at = now() WHERE id = $1 AND deleted_at IS NULL
             RETURNING id, hostname, os, arch, platform, tags, conn_mode, addr, public_ip, local_ips",
        )
        .bind(id)
        .bind(tags)
        .fetch_optional(self.db.pool())
        .await
    }

    /// 软删除主机，返回受影响行数。
    pub async fn soft_delete(&self, id: Uuid) -> sqlx::Result<u64> {
        sqlx::query("UPDATE hosts SET deleted_at = now() WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await
            .map(|r| r.rows_affected())
    }
}
