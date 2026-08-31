//! gRPC 适配层：Agent 反向连入的 AgentService 实现与连接注册表。

pub mod agent_service;
pub mod connection_registry;
pub mod transfer_registry;

use crate::config::Config;
use crate::store::Db;
use agent_service::AgentServiceImpl;
use connection_registry::ConnectionRegistry;
use helm_proto::pb::agent_service_server::AgentServiceServer;
use transfer_registry::TransferRegistry;

/// 启动 gRPC 服务（Agent 反向连入）。
pub async fn serve(
    config: Config,
    db: Db,
    registry: ConnectionRegistry,
    transfers: TransferRegistry,
) -> anyhow::Result<()> {
    let addr = config.grpc_addr.parse()?;
    let svc = AgentServiceServer::new(AgentServiceImpl::new(
        registry,
        transfers,
        db,
        config.server_token,
    ));

    tracing::info!(addr = %config.grpc_addr, "grpc listening");
    tonic::transport::Server::builder()
        .add_service(svc)
        .serve(addr)
        .await?;
    Ok(())
}
