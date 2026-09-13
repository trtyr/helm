//! 应用层：进程管理 + 网络信息采集。

use crate::domain::{Error, Result};
use crate::grpc::connection_registry::ConnectionRegistry;
use crate::grpc::query_registry::{QueryRegistry, QueryResponse};
use helm_proto::pb::{
    AutorunsAction, FileMetaQuery, FileMetaResult, FsTimelineQuery, IrScan, MemScan, NetInfo,
    NetInfoResult, ProcessInfo, ProcessKill, ProcessList, ServerMessage, SysServiceAction,
    SysServiceList, SysServiceListResult, server_message,
};
use uuid::Uuid;

const QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// 内存扫描 / 应急扫描需要更长时间。
const SCAN_QUERY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// 进程/网络查询用例。
#[derive(Clone)]
pub struct ProcessService {
    registry: ConnectionRegistry,
    query: QueryRegistry,
}

impl ProcessService {
    pub fn new(registry: ConnectionRegistry, query: QueryRegistry) -> Self {
        Self { registry, query }
    }

    /// 下发一次查询并等待结果（默认超时）。
    async fn request(
        &self,
        agent_id: &str,
        request_id: &str,
        kind: server_message::Kind,
    ) -> Result<QueryResponse> {
        self.request_with_timeout(agent_id, request_id, kind, QUERY_TIMEOUT)
            .await
    }

    /// 下发一次查询并等待结果（自定义超时，用于耗时操作）。
    async fn request_with_timeout(
        &self,
        agent_id: &str,
        request_id: &str,
        kind: server_message::Kind,
        timeout: std::time::Duration,
    ) -> Result<QueryResponse> {
        let rx = self.query.register(request_id.to_string()).await;
        let msg = ServerMessage { kind: Some(kind) };
        self.registry
            .send(agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))?;
        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| Error::Internal("query timeout".into()))?
            .map_err(|_| Error::Internal("query channel closed".into()))
    }

    /// 列出进程。
    pub async fn list(&self, agent_id: &str) -> Result<Vec<ProcessInfo>> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::ProcessList(ProcessList {
                    request_id: request_id.clone(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::ProcessList(r) => Ok(r.processes),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 终止进程。
    pub async fn kill(&self, agent_id: &str, pid: i32) -> Result<bool> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::ProcessKill(ProcessKill {
                    request_id: request_id.clone(),
                    pid,
                }),
            )
            .await?;
        match resp {
            QueryResponse::ProcessKill(r) => Ok(r.ok),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 采集网络信息。
    pub async fn net(&self, agent_id: &str) -> Result<NetInfoResult> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::NetInfo(NetInfo {
                    request_id: request_id.clone(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::NetInfo(r) => Ok(r),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 枚举目标机系统服务。
    pub async fn sys_services(&self, agent_id: &str) -> Result<SysServiceListResult> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::SysServiceList(SysServiceList {
                    request_id: request_id.clone(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::SysServiceList(r) => Ok(r),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 应急扫描（账户/启动项/事件/可疑文件）。
    pub async fn ir_scan(&self, agent_id: &str, types: &[String]) -> Result<IrScanResultView> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request_with_timeout(
                agent_id,
                &request_id,
                server_message::Kind::IrScan(IrScan {
                    request_id: request_id.clone(),
                    types: types.to_vec(),
                    log_name: String::new(),
                    event_ids: String::new(),
                    event_count: 0,
                }),
                SCAN_QUERY_TIMEOUT,
            )
            .await?;
        match resp {
            QueryResponse::IrScan(r) => Ok(IrScanResultView::from(r)),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 进程内存字符串扫描。
    pub async fn mem_scan(
        &self,
        agent_id: &str,
        pid: i32,
        min_len: u32,
        keyword: &str,
    ) -> Result<MemScanResultView> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request_with_timeout(
                agent_id,
                &request_id,
                server_message::Kind::MemScan(helm_proto::pb::MemScan {
                    request_id: request_id.clone(),
                    pid,
                    min_len,
                    keywords: keyword.to_string(),
                    timeout_secs: 30,
                    stream: false,
                }),
                SCAN_QUERY_TIMEOUT,
            )
            .await?;
        match resp {
            QueryResponse::MemScan(r) => Ok(MemScanResultView::from(r)),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 系统服务操作（start / stop / restart）。
    pub async fn sys_service_action(
        &self,
        agent_id: &str,
        name: &str,
        action: &str,
    ) -> Result<(bool, Option<String>)> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::SysServiceAction(SysServiceAction {
                    request_id: request_id.clone(),
                    name: name.to_string(),
                    action: action.to_string(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::SysServiceAction(r) => Ok((r.ok, r.error)),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }
    /// NTFS USN 文件时间线。
    pub async fn fs_timeline(
        &self,
        agent_id: &str,
        drive: &str,
        since_hours: u32,
        limit: u32,
        keyword: &str,
    ) -> Result<helm_proto::pb::FsTimelineResult> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request_with_timeout(
                agent_id,
                &request_id,
                server_message::Kind::FsTimelineQuery(FsTimelineQuery {
                    request_id: request_id.clone(),
                    drive: drive.to_string(),
                    since_hours,
                    limit,
                    keyword: keyword.to_string(),
                }),
                SCAN_QUERY_TIMEOUT,
            )
            .await?;
        match resp {
            QueryResponse::FsTimeline(r) => Ok(r),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 启动流式内存扫描（只下发不等待；批次经 StreamRegistry 广播，最终帧完成查询）。
    pub async fn mem_scan_start(
        &self,
        agent_id: &str,
        scan_id: &str,
        pid: i32,
        min_len: u32,
        keyword: &str,
    ) -> Result<()> {
        let msg = ServerMessage {
            kind: Some(server_message::Kind::MemScan(MemScan {
                request_id: scan_id.to_string(),
                pid,
                min_len,
                keywords: keyword.to_string(),
                timeout_secs: 0,
                stream: true,
            })),
        };
        self.registry
            .send(agent_id, msg)
            .await
            .map_err(|e| Error::NotConnected(e.to_string()))
    }

    /// 启动项操作（禁用/启用/删除）。
    pub async fn autoruns_action(
        &self,
        agent_id: &str,
        action: &str,
        op_key: &str,
    ) -> Result<(bool, Option<String>)> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::AutorunsAction(AutorunsAction {
                    request_id: request_id.clone(),
                    action: action.to_string(),
                    op_key: op_key.to_string(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::AutorunsAction(r) => Ok((r.ok, r.error)),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }

    /// 文件元数据按需查询（SHA256/大小/mtime）。
    pub async fn file_meta(&self, agent_id: &str, path: &str) -> Result<FileMetaResult> {
        let request_id = Uuid::new_v4().to_string();
        let resp = self
            .request(
                agent_id,
                &request_id,
                server_message::Kind::FileMetaQuery(FileMetaQuery {
                    request_id: request_id.clone(),
                    path: path.to_string(),
                }),
            )
            .await?;
        match resp {
            QueryResponse::FileMeta(r) => Ok(r),
            _ => Err(Error::Internal("unexpected query response".into())),
        }
    }
}

/// IR 扫描结果视图（HTTP 序列化）。
pub struct IrScanResultView {
    pub findings: Vec<serde_json::Value>,
    pub error: Option<String>,
}

impl From<helm_proto::pb::IrScanResult> for IrScanResultView {
    fn from(r: helm_proto::pb::IrScanResult) -> Self {
        Self {
            findings: r
                .findings
                .into_iter()
                .map(|f| {
                    serde_json::json!({
                        "category": f.category,
                        "name": f.name,
                        "detail": f.detail,
                        "severity": f.severity,
                        "path": f.path,
                        "publisher": f.publisher,
                        "signState": f.sign_state,
                        "desc": f.desc,
                        "opKey": f.op_key,
                        "disabled": f.disabled,
                        "mtime": f.mtime,
                        "tsUnix": f.ts_unix,
                    })
                })
                .collect(),
            error: r.error,
        }
    }
}

/// 内存扫描结果视图。
pub struct MemScanResultView {
    pub pid: i32,
    pub matches: Vec<String>,
    pub scanned_bytes: u64,
    pub truncated: bool,
    pub error: Option<String>,
    pub pids_total: u32,
    pub pids_scanned: u32,
}

impl From<helm_proto::pb::MemScanResult> for MemScanResultView {
    fn from(r: helm_proto::pb::MemScanResult) -> Self {
        Self {
            pid: r.pid,
            matches: r.matches,
            scanned_bytes: r.scanned_bytes,
            truncated: r.truncated,
            error: r.error,
            pids_total: r.pids_total,
            pids_scanned: r.pids_scanned,
        }
    }
}
