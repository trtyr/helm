//! 监听器运行时注册表：管理多个 gRPC 监听器的启动与优雅停止。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::grpc::agent_service::AgentServiceImpl;
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use crate::store::listener_repo::ListenerRow;
use helm_proto::pb::agent_service_server::AgentServiceServer;
use tokio::sync::{Mutex, oneshot};
use uuid::Uuid;

/// 监听器运行时错误。
#[derive(Debug, thiserror::Error)]
pub enum ListenerError {
    #[error("listener already running: {0}")]
    AlreadyRunning(Uuid),
    #[error("listener not running: {0}")]
    NotRunning(Uuid),
    #[error("invalid listen addr: {0}")]
    InvalidAddr(String),
}

/// 监听器运行时管理器：每个运行中的监听器对应一个 shutdown 句柄。
#[derive(Clone, Default)]
pub struct ListenerRegistry {
    inner: Arc<Mutex<HashMap<Uuid, oneshot::Sender<()>>>>,
}

impl ListenerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// 启动一个监听器：spawn gRPC server task 并登记 shutdown 句柄。
    ///
    /// `token` 为该监听器的认证 token（调用方已处理 auth 为空时的回退）。
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        &self,
        listener: &ListenerRow,
        registry: ConnectionRegistry,
        transfers: TransferRegistry,
        db: Db,
        token: String,
    ) -> Result<(), ListenerError> {
        let mut map = self.inner.lock().await;
        if map.contains_key(&listener.id) {
            return Err(ListenerError::AlreadyRunning(listener.id));
        }
        let addr: SocketAddr = listener
            .addr
            .parse()
            .map_err(|_| ListenerError::InvalidAddr(listener.addr.clone()))?;
        let svc = AgentServiceServer::new(AgentServiceImpl::new(registry, transfers, db, token));
        let (tx, rx) = oneshot::channel::<()>();
        let id = listener.id;
        let addr_str = listener.addr.clone();
        tokio::spawn(async move {
            tracing::info!(listener_id = %id, addr = %addr_str, "listener started");
            let _ = tonic::transport::Server::builder()
                .add_service(svc)
                .serve_with_shutdown(addr, async move {
                    let _ = rx.await;
                })
                .await;
            tracing::info!(listener_id = %id, "listener stopped");
        });
        map.insert(id, tx);
        Ok(())
    }

    /// 停止一个监听器：触发 shutdown 并移除句柄。
    pub async fn stop(&self, id: Uuid) -> Result<(), ListenerError> {
        let tx = self
            .inner
            .lock()
            .await
            .remove(&id)
            .ok_or(ListenerError::NotRunning(id))?;
        let _ = tx.send(());
        Ok(())
    }

    /// 监听器是否在运行。
    pub async fn is_running(&self, id: Uuid) -> bool {
        self.inner.lock().await.contains_key(&id)
    }
}
