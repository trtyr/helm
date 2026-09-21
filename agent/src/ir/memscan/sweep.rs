//! 全进程内存扫描（G11 拆分，2026-09-21）——自 `memscan.rs` 拆出：
//! 进程枚举、逐进程扫描编排、全局命中配额、流式帧构造。
//!
//! 单进程扫描本体（`mem_scan_impl`）与对外入口仍留在父模块；本模块只负责
//! 「扫哪些进程、结果怎么裁、帧怎么发」。

use super::mem_scan_impl;
use helm_proto::pb::{AgentMessage, MemMatch, MemScanResult, agent_message};

/// 全进程模式的全局限额（无截止时间，扫完为止；进度经流式实时可见）。
const ALL_MAX_MATCHES: usize = 5000;
const ALL_MAX_HITS: usize = 5000;
const PER_PROCESS_MATCH_CAP: usize = 100;

/// 全进程模式：枚举进程逐一扫描，带全局命中限额与总超时。
/// partial = Some(tx) 时流式：每扫完一个进程推一帧部分批次（finished=false），结束帧 finished=true。
///
/// **G11 拆分（2026-09-21）**：原为 184 行单块（枚举 + 逐进程扫描 + 配额截断 + 流式推送 +
/// 结束帧全在一处）。现拆为——`enum_processes`（枚举）/ [`MatchBudget`]（配额与截断）/
/// `mem_frame`（帧构造）/ `mem_frame_error`（失败帧）；本函数只做阶段编排。
pub(super) async fn mem_scan_all_inner(
    request_id: &str,
    min_len: usize,
    keyword: &str,
    partial: Option<&tokio::sync::mpsc::Sender<AgentMessage>>,
) -> AgentMessage {
    // 阶段 1：枚举进程（排除 pid 0 与自身）
    let Some(procs) = enum_processes() else {
        return mem_frame_error(request_id, "枚举进程失败（CreateToolhelp32Snapshot）");
    };
    let mut progress = SweepProgress {
        total: procs.len() as u32,
        ..Default::default()
    };

    // 阶段 2：逐进程扫描；配额满后继续扫描统计覆盖，只是不再收集字符串
    let mut budget = MatchBudget::default();
    for (pid, name) in procs {
        let tag = format!("[{pid} {name}] ");
        tracing::debug!(pid, "mem_scan_all: scanning process");
        let Ok((matches, hits, scanned, _)) = mem_scan_impl(pid as i32, min_len, keyword) else {
            // OpenProcess 失败（受保护/系统进程）直接跳过，不计失败
            continue;
        };
        progress.scanned_bytes += scanned;
        progress.scanned_n += 1;
        let (batch_matches, batch_hits) = budget.absorb(&tag, matches, hits);
        progress.truncated = budget.truncated;
        if let Some(tx) = partial {
            let msg = mem_frame(
                request_id,
                false,
                pid as i32,
                batch_matches,
                batch_hits,
                &progress,
            );
            // 订阅端消失（WS 断开）则终止扫描
            if tx.send(msg).await.is_err() {
                break;
            }
        }
    }

    // 阶段 3：结束帧（流式：结果已逐帧发出，结束帧不带批次；非流式：带上全部结果）
    tracing::info!(
        pids_scanned = progress.scanned_n,
        total = progress.total,
        truncated = progress.truncated,
        timed_out = progress.timed_out,
        "mem_scan_all: sweep done, sending final frame"
    );
    let (final_matches, final_hits) = if partial.is_some() {
        (Vec::new(), Vec::new())
    } else {
        budget.into_parts()
    };
    let final_msg = mem_frame(request_id, true, 0, final_matches, final_hits, &progress);
    match partial {
        Some(tx) => {
            match tx.send(final_msg).await {
                Ok(()) => tracing::info!("mem_scan_all: final frame sent"),
                Err(e) => tracing::warn!(error = %e, "mem_scan_all: final frame send FAILED"),
            }
            AgentMessage { kind: None }
        }
        None => final_msg,
    }
}

/// 跨帧携带的扫描进度（每帧回填给控制台）。
#[derive(Clone, Copy, Default)]
struct SweepProgress {
    scanned_bytes: u64,
    scanned_n: u32,
    /// 待扫进程总数
    total: u32,
    truncated: bool,
    timed_out: bool,
}

/// 全局命中配额：超额即截断，且配额满后不再收集（只继续统计覆盖率）。
#[derive(Default)]
struct MatchBudget {
    matches: Vec<String>,
    hits: Vec<MemMatch>,
    truncated: bool,
}

impl MatchBudget {
    /// 两个配额都未满时才值得收集字符串。
    fn can_collect(&self) -> bool {
        self.matches.len() < ALL_MAX_MATCHES && self.hits.len() < ALL_MAX_HITS
    }

    /// 收下一个进程的结果：加 `[pid name] ` 前缀 + 每进程上限 + 全局上限，返回本批可上报内容。
    fn absorb(
        &mut self,
        tag: &str,
        matches: Vec<String>,
        hits: Vec<MemMatch>,
    ) -> (Vec<String>, Vec<MemMatch>) {
        if !self.can_collect() {
            return (Vec::new(), Vec::new());
        }
        let mut batch_matches: Vec<String> = Vec::new();
        for s in matches.into_iter().take(PER_PROCESS_MATCH_CAP) {
            if self.matches.len() + batch_matches.len() >= ALL_MAX_MATCHES {
                self.truncated = true;
                break;
            }
            batch_matches.push(format!("{}{}", tag, super::ffi::truncate_utf8(&s, 180)));
        }
        let mut batch_hits: Vec<MemMatch> = Vec::new();
        for h in hits.into_iter().take(PER_PROCESS_MATCH_CAP) {
            if self.hits.len() + batch_hits.len() >= ALL_MAX_HITS {
                self.truncated = true;
                break;
            }
            batch_hits.push(MemMatch {
                addr: format!("{}{}", tag, h.addr),
                kind: h.kind,
                value: h.value,
            });
        }
        if self.matches.len() + batch_matches.len() >= ALL_MAX_MATCHES
            || self.hits.len() + batch_hits.len() >= ALL_MAX_HITS
        {
            self.truncated = true;
        }
        self.matches.extend(batch_matches.iter().cloned());
        self.hits.extend(batch_hits.iter().cloned());
        (batch_matches, batch_hits)
    }

    /// 取走全部累计结果（非流式收尾用）。
    fn into_parts(self) -> (Vec<String>, Vec<MemMatch>) {
        (self.matches, self.hits)
    }
}

/// 构造一帧结果（流式：每进程一帧 + 结束帧；非流式：只有结束帧且带全部结果）。
fn mem_frame(
    request_id: &str,
    finished: bool,
    pid: i32,
    matches: Vec<String>,
    hits: Vec<MemMatch>,
    progress: &SweepProgress,
) -> AgentMessage {
    AgentMessage {
        kind: Some(agent_message::Kind::MemScanResult(MemScanResult {
            request_id: request_id.to_string(),
            pid,
            matches,
            hits,
            scanned_bytes: progress.scanned_bytes,
            truncated: progress.truncated,
            regions_total: 0,
            regions_scanned: 0,
            timed_out: progress.timed_out,
            error: None,
            pids_total: progress.total,
            pids_scanned: progress.scanned_n,
            finished,
        })),
    }
}

/// 枚举失败时的单帧回包（finished=true + error 文案，进度全零）。
fn mem_frame_error(request_id: &str, msg: &str) -> AgentMessage {
    let mut frame = mem_frame(
        request_id,
        true,
        0,
        Vec::new(),
        Vec::new(),
        &SweepProgress::default(),
    );
    if let Some(agent_message::Kind::MemScanResult(res)) = frame.kind.as_mut() {
        res.error = Some(msg.to_string());
    }
    frame
}

/// 阶段 1：枚举进程快照（排除 pid 0 与自身进程）；快照创建失败返回 `None`。
fn enum_processes() -> Option<Vec<(u32, String)>> {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() {
            return None;
        }
        let mut procs: Vec<(u32, String)> = Vec::new();
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            cntUsage: 0,
            th32ProcessID: 0,
            th32DefaultHeapID: 0,
            th32ModuleID: 0,
            cntThreads: 0,
            th32ParentProcessID: 0,
            pcPriClassBase: 0,
            dwFlags: 0,
            szExeFile: [0; 260],
        };
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let pid = entry.th32ProcessID;
                if pid != 0 && pid != std::process::id() {
                    let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                    procs.push((pid, String::from_utf16_lossy(&entry.szExeFile[..end])));
                }
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        windows_sys::Win32::Foundation::CloseHandle(snap);
        Some(procs)
    }
}
