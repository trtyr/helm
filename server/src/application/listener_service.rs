//! 应用层：监听器编排（创建 / 列表 / 启停 / 重启恢复）。

use crate::application::cert_service::CertService;
use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::listener_registry::ListenerRegistry;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use crate::store::listener_repo::{ListenerRepo, ListenerRow};
use serde::Serialize;
use uuid::Uuid;

/// 监听器视图（不含 auth 密钥）。
#[derive(Debug, Serialize)]
pub struct ListenerView {
    pub id: Uuid,
    pub name: String,
    pub addr: String,
    pub proto: String,
    pub status: String,
    pub running: bool,
}

impl ListenerView {
    fn from_row(row: ListenerRow, running: bool) -> Self {
        Self {
            id: row.id,
            name: row.name,
            addr: row.addr,
            proto: row.proto,
            status: row.status,
            running,
        }
    }
}

/// 监听器用例。
#[derive(Clone)]
pub struct ListenerService {
    db: Db,
    listeners: ListenerRegistry,
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
    sessions: SessionRegistry,
    file_list: FileListRegistry,
    query: QueryRegistry,
    streams: StreamRegistry,
    metrics: crate::application::metric_sink::MetricSink,
    fallback_token: String,
    cert: CertService,
}

impl ListenerService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db: Db,
        listeners: ListenerRegistry,
        registry: ConnectionRegistry,
        transfers: TransferRegistry,
        sessions: SessionRegistry,
        file_list: FileListRegistry,
        query: QueryRegistry,
        streams: StreamRegistry,
        metrics: crate::application::metric_sink::MetricSink,
        fallback_token: String,
        cert: CertService,
    ) -> Self {
        Self {
            db,
            listeners,
            registry,
            transfers,
            sessions,
            file_list,
            query,
            streams,
            metrics,
            fallback_token,
            cert,
        }
    }

    /// 创建监听器（默认 stopped，不自动启动）。
    pub async fn create(
        &self,
        name: &str,
        addr: &str,
        proto: &str,
        auth: &str,
    ) -> Result<ListenerView> {
        let row = ListenerRepo::new(self.db.clone())
            .create(name, addr, proto, auth)
            .await?;
        Ok(ListenerView::from_row(row, false))
    }

    /// 列出所有监听器，附运行态。
    pub async fn list(&self) -> Result<Vec<ListenerView>> {
        let rows = ListenerRepo::new(self.db.clone()).list().await?;
        let mut views = Vec::with_capacity(rows.len());
        for row in rows {
            let running = self.listeners.is_running(row.id).await;
            views.push(ListenerView::from_row(row, running));
        }
        Ok(views)
    }

    /// 启动监听器：auth 为空时回退到全局 token。
    pub async fn start(&self, id: Uuid) -> Result<()> {
        let row = self
            .get(id)
            .await?
            .ok_or_else(|| Error::NotFound(format!("listener: {id}")))?;
        let token = if row.auth.is_empty() {
            self.fallback_token.clone()
        } else {
            row.auth.clone()
        };
        self.listeners
            .start(
                &row,
                self.registry.clone(),
                self.transfers.clone(),
                self.sessions.clone(),
                self.file_list.clone(),
                self.query.clone(),
                self.streams.clone(),
                self.metrics.clone(),
                self.db.clone(),
                token,
                self.cert.clone(),
            )
            .await
            .map_err(|e| Error::InvalidArgument(e.to_string()))?;
        ListenerRepo::new(self.db.clone())
            .set_status(id, "running")
            .await?;
        Ok(())
    }

    /// 停止监听器。
    pub async fn stop(&self, id: Uuid) -> Result<()> {
        if self.listeners.is_running(id).await {
            self.listeners
                .stop(id)
                .await
                .map_err(|e| Error::InvalidArgument(e.to_string()))?;
        }
        ListenerRepo::new(self.db.clone())
            .set_status(id, "stopped")
            .await?;
        Ok(())
    }

    /// 更新监听器（name/addr/proto/auth）。
    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        addr: &str,
        proto: &str,
        auth: &str,
    ) -> Result<ListenerView> {
        let row = ListenerRepo::new(self.db.clone())
            .update(id, name, addr, proto, auth)
            .await?
            .ok_or_else(|| Error::NotFound(format!("listener: {id}")))?;
        let running = self.listeners.is_running(id).await;
        Ok(ListenerView::from_row(row, running))
    }

    /// 删除监听器（先停再删）。
    pub async fn delete(&self, id: Uuid) -> Result<()> {
        if self.listeners.is_running(id).await {
            let _ = self.listeners.stop(id).await;
        }
        ListenerRepo::new(self.db.clone()).delete(id).await?;
        Ok(())
    }

    /// 重启恢复：恢复所有 running 监听器；首次启动（表为空）时 seed 默认监听器。
    pub async fn resume_or_seed(&self, default_addr: &str) -> Result<()> {
        let repo = ListenerRepo::new(self.db.clone());
        let running = repo.list_running().await?;
        if running.is_empty() {
            if repo.count().await? == 0 {
                let row = repo.create("default", default_addr, "grpc", "").await?;
                let token = self.fallback_token.clone();
                self.listeners
                    .start(
                        &row,
                        self.registry.clone(),
                        self.transfers.clone(),
                        self.sessions.clone(),
                        self.file_list.clone(),
                        self.query.clone(),
                        self.streams.clone(),
                        self.metrics.clone(),
                        self.db.clone(),
                        token,
                        self.cert.clone(),
                    )
                    .await
                    .map_err(|e| Error::Internal(e.to_string()))?;
                repo.set_status(row.id, "running").await?;
                tracing::info!(listener_id = %row.id, name = "default", "seeded default listener");
            }
            return Ok(());
        }
        for row in running {
            let token = if row.auth.is_empty() {
                self.fallback_token.clone()
            } else {
                row.auth.clone()
            };
            match self
                .listeners
                .start(
                    &row,
                    self.registry.clone(),
                    self.transfers.clone(),
                    self.sessions.clone(),
                    self.file_list.clone(),
                    self.query.clone(),
                    self.streams.clone(),
                    self.metrics.clone(),
                    self.db.clone(),
                    token,
                    self.cert.clone(),
                )
                .await
            {
                Ok(()) => tracing::info!(listener_id = %row.id, "resumed listener"),
                Err(e) => {
                    tracing::warn!(listener_id = %row.id, error = %e, "failed to resume listener");
                    let _ = ListenerRepo::new(self.db.clone())
                        .set_status(row.id, "stopped")
                        .await;
                }
            }
        }
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<ListenerRow>> {
        Ok(ListenerRepo::new(self.db.clone()).get(id).await?)
    }
}
