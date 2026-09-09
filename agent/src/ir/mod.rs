//! 应急响应（IR）采集：对标 Sysinternals Autoruns / 企业 IR 手册。
//!
//! 扫描类型（ir_scan 的 types 过滤）：
//! - `accounts`        账户审计（隐藏账户/克隆账户/管理员组）
//! - `autostart`       Autoruns 全景（登录/服务/驱动/计划任务/浏览器/外壳/WMI 订阅/映像劫持）
//! - `registry`        注册表安全基线（映像劫持/认证提供程序/Winlogon 劫持）
//! - `events`          事件日志（4624/4625/4720/1102/7045/4104）
//! - `suspicious_files` 可疑落地文件 + hosts
//!
//! 内存字符串扫描：mem_scan（SeDebug + VirtualQueryEx + ASCII/UTF-16LE 提取）。
//!
//! Windows 原生实现；其他平台返回明确错误。

use helm_proto::pb::{AgentMessage, IrScanResult, agent_message};

#[cfg(windows)]
mod accounts;
#[cfg(windows)]
mod actions;
#[cfg(windows)]
mod autostart;
#[cfg(windows)]
mod browser;
#[cfg(windows)]
mod events;
#[cfg(windows)]
mod files;
#[cfg(windows)]
mod fs_timeline;
#[cfg(windows)]
mod filemeta;
#[cfg(windows)]
mod hijacks;
#[cfg(windows)]
mod lsa;
#[cfg(windows)]
mod memscan;
#[cfg(windows)]
mod office;
#[cfg(windows)]
mod services;
#[cfg(windows)]
mod system;
#[cfg(windows)]
mod tasks;
#[cfg(windows)]
mod util;
#[cfg(windows)]
mod wmi;

/// 执行应急扫描。`types` 为空 = 全部类型。
pub fn ir_scan(request_id: &str, types: &[String]) -> AgentMessage {
    let want = |t: &str| types.is_empty() || types.iter().any(|x| x == t);

    #[cfg(windows)]
    let mut findings = {
        let mut sc = util::Scanner::new();
        if want("accounts") {
            accounts::scan(&mut sc);
        }
        if want("autostart") || want("registry") {
            hijacks::scan(&mut sc); // 两类扫描共享，重复条目由 Scanner 去重
        }
        if want("autostart") {
            // Autoruns 全景：登录/外壳 → 服务/驱动 → 计划任务 → WMI → 浏览器 → 系统级 → Office
            autostart::scan(&mut sc);
            services::scan(&mut sc);
            tasks::scan(&mut sc);
            wmi::scan(&mut sc);
            browser::scan(&mut sc);
            system::scan(&mut sc);
            office::scan(&mut sc);
        }
        if want("registry") {
            // 注册表安全基线：认证提供程序
            lsa::scan(&mut sc);
        }
        if want("events") {
            events::scan(&mut sc);
        }
        if want("suspicious_files") {
            files::scan(&mut sc);
        }
        sc.done()
    };

    #[cfg(not(windows))]
    let mut findings = {
        let _ = want;
        vec![helm_proto::pb::IrFinding {
            category: "平台".into(),
            name: "不支持".into(),
            detail: "应急扫描当前仅支持 Windows 主机".into(),
            severity: "info".into(),
            path: None,
            publisher: None,
            sign_state: None,
            desc: None,
            op_key: None,
            disabled: None,
            mtime: None,
            ts_unix: None,
        }]
    };

    findings.truncate(util_max());
    AgentMessage {
        kind: Some(agent_message::Kind::IrScanResult(IrScanResult {
            request_id: request_id.to_string(),
            findings,
            error: None,
        })),
    }
}

#[cfg(windows)]
fn util_max() -> usize {
    util::MAX_FINDINGS
}
#[cfg(not(windows))]
fn util_max() -> usize {
    4000
}

/// 进程内存字符串扫描。
pub async fn mem_scan(request_id: &str, pid: i32, min_len: u32, keyword: &str) -> AgentMessage {
    #[cfg(windows)]
    {
        memscan::mem_scan(request_id, pid, min_len, keyword).await
    }
    #[cfg(not(windows))]
    {
        let _ = (pid, min_len, keyword);
        AgentMessage {
            kind: Some(agent_message::Kind::MemScanResult(helm_proto::pb::MemScanResult {
                request_id: request_id.to_string(),
                pid,
                matches: vec![],
                hits: vec![],
                scanned_bytes: 0,
                truncated: false,
                regions_total: 0,
                regions_scanned: 0,
                timed_out: false,
                error: Some("内存扫描当前仅支持 Windows 主机".into()),
                pids_total: 1,
                pids_scanned: 0,
                finished: true,
            })),
        }
    }
}

/// 流式内存扫描：逐进程推部分批次到 tx，结束帧 finished=true。
pub async fn mem_scan_stream(
    request_id: String,
    pid: i32,
    min_len: u32,
    keyword: String,
    tx: tokio::sync::mpsc::Sender<AgentMessage>,
) {
    #[cfg(windows)]
    {
        memscan::mem_scan_stream(request_id, pid, min_len, keyword, tx).await;
    }
    #[cfg(not(windows))]
    {
        let _ = (&pid, &min_len, &keyword, &tx);
    }
}

/// 启动项操作（禁用/启用/删除，AutorunsDisabled 机制）。
pub fn autoruns_action(request_id: &str, action: &str, op_key: &str) -> AgentMessage {
    #[cfg(windows)]
    {
        actions::autoruns_action(request_id, action, op_key)
    }
    #[cfg(not(windows))]
    {
        let _ = (action, op_key);
        AgentMessage {
            kind: Some(agent_message::Kind::AutorunsActionResult(
                helm_proto::pb::AutorunsActionResult {
                    request_id: request_id.to_string(),
                    ok: false,
                    error: Some("启动项操作当前仅支持 Windows 主机".into()),
                },
            )),
        }
    }
}

/// 文件元数据按需查询（SHA256/大小/mtime）。
pub fn file_meta(request_id: &str, path: &str) -> AgentMessage {
    #[cfg(windows)]
    {
        filemeta::file_meta(request_id, path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        AgentMessage {
            kind: Some(agent_message::Kind::FileMetaResult(helm_proto::pb::FileMetaResult {
                request_id: request_id.to_string(),
                path: path.to_string(),
                sha256: String::new(),
                size: 0,
                mtime: String::new(),
                error: Some("仅支持 Windows 主机".into()),
            })),
        }
    }
}

/// NTFS USN 文件时间线。
pub fn fs_timeline(request_id: &str, drive: &str, since_hours: u32, limit: u32, keyword: &str) -> AgentMessage {
    #[cfg(windows)]
    {
        fs_timeline::fs_timeline(request_id, drive, since_hours, limit, keyword)
    }
    #[cfg(not(windows))]
    {
        let _ = (drive, since_hours, limit, keyword);
        AgentMessage {
            kind: Some(agent_message::Kind::FsTimelineResult(helm_proto::pb::FsTimelineResult {
                request_id: request_id.to_string(),
                drive: drive.to_string(),
                entries: vec![],
                total_scanned: 0,
                truncated: false,
                error: Some("仅支持 Windows 主机".into()),
            })),
        }
    }
}
