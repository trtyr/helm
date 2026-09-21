//! 监听器运行时注册表：管理多个 gRPC 监听器的启动与优雅停止。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use crate::application::cert_service::CertService;
use crate::grpc::agent_service::{AgentServiceDeps, AgentServiceImpl};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::file_list_registry::FileListRegistry;
use crate::grpc::query_registry::QueryRegistry;
use crate::grpc::session_registry::SessionRegistry;
use crate::grpc::stream_registry::StreamRegistry;
use crate::grpc::transfer_registry::TransferRegistry;
use crate::store::Db;
use crate::store::listener_repo::ListenerRow;
use helm_proto::pb::agent_service_server::AgentServiceServer;
use tokio::sync::{Mutex, oneshot};
use tonic::transport::{Certificate, Identity, ServerTlsConfig};
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
    /// `server_tokens` 为该监听器可接受的认证 token 全集（调用方已处理 auth 为空时的回退）。
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        &self,
        listener: &ListenerRow,
        registry: ConnectionRegistry,
        transfers: TransferRegistry,
        sessions: SessionRegistry,
        file_list: FileListRegistry,
        query: QueryRegistry,
        streams: StreamRegistry,
        metrics: crate::application::metric_sink::MetricSink,
        db: Db,
        server_tokens: Vec<String>,
        cert: CertService,
    ) -> Result<(), ListenerError> {
        let mut map = self.inner.lock().await;
        if map.contains_key(&listener.id) {
            return Err(ListenerError::AlreadyRunning(listener.id));
        }
        let addr: SocketAddr = listener
            .addr
            .parse()
            .map_err(|_| ListenerError::InvalidAddr(listener.addr.clone()))?;
        let svc = AgentServiceServer::new(AgentServiceImpl::new(AgentServiceDeps {
            registry,
            transfers,
            sessions,
            file_list,
            query,
            streams,
            metrics,
            db,
            server_tokens,
        }));
        let (tx, rx) = oneshot::channel::<()>();
        let id = listener.id;
        let addr_str = listener.addr.clone();
        let cert = cert.clone();
        tokio::spawn(async move {
            tracing::info!(listener_id = %id, addr = %addr_str, "listener started");
            let mut builder = tonic::transport::Server::builder()
                // EN-72 对称加固：server 端 h2 keepalive——对半开连接 40s 内
                // 主动判死并回收，不再依赖 TCP 被动超时；与 agent 端
                // keepalive（commit 4d282c5）对称，双向快速检测。
                .http2_keepalive_interval(Some(std::time::Duration::from_secs(30)))
                .http2_keepalive_timeout(Some(std::time::Duration::from_secs(10)));
            if cert.enabled() {
                let tls = ServerTlsConfig::new()
                    .identity(Identity::from_pem(
                        cert.server_cert_pem(),
                        cert.server_key_pem(),
                    ))
                    .client_ca_root(Certificate::from_pem(cert.ca_cert_pem()));
                builder = match builder.tls_config(tls) {
                    Ok(b) => b,
                    Err(e) => {
                        tracing::error!(listener_id = %id, error = %e, "tls config failed");
                        return;
                    }
                };
            }
            let server = builder.add_service(svc);
            // 监听器退出即服务不可用：serve 的 Err 必须可见（此前被静默丢弃）
            if let Err(e) = server
                .serve_with_shutdown(addr, async move {
                    // 关闭信号：发送端（stop）可能已消失，取消即视为已停止
                    let _ = rx.await;
                })
                .await
            {
                tracing::error!(listener_id = %id, error = %e, "listener serve failed");
            }
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
        let _ = tx.send(()); // 停止信号：接收侧（serve 任务）已退出即无需再送
        Ok(())
    }

    /// 监听器是否在运行。
    pub async fn is_running(&self, id: Uuid) -> bool {
        self.inner.lock().await.contains_key(&id)
    }
}
