//! 进程内存字符串扫描：VirtualQueryEx + ReadProcessMemory + ASCII/UTF-16LE 提取
//! （Volatility strings 简化版）。pid=0 时遍历全部进程（全局限额 + 超时），
//! 流式模式下逐进程推送部分批次。

use helm_proto::pb::{AgentMessage, MemMatch, MemScanResult, agent_message};

mod ffi;

use ffi::{enable_debug_privilege, truncate_utf8};
pub use ffi::{extract_ascii_strings, extract_utf16_strings};

mod sweep;

use sweep::mem_scan_all_inner;

pub async fn mem_scan(request_id: &str, pid: i32, min_len: u32, keyword: &str) -> AgentMessage {
    let min_len = if min_len == 0 { 6 } else { min_len as usize };
    // 提权失败不中断扫描（部分进程读不到），但必须留痕：否则「扫不到 SYSTEM 进程」无从解释
    if !enable_debug_privilege() {
        tracing::warn!("SeDebugPrivilege not enabled: system-owned processes may be unreadable");
    }
    if pid == 0 {
        return mem_scan_all_inner(request_id, min_len, keyword, None).await;
    }
    mem_scan_single(request_id, pid, min_len, keyword)
}

/// 流式扫描：逐进程推部分批次（finished=false），结束帧 finished=true。
pub async fn mem_scan_stream(
    request_id: String,
    pid: i32,
    min_len: u32,
    keyword: String,
    tx: tokio::sync::mpsc::Sender<AgentMessage>,
) {
    let min_len = if min_len == 0 { 6 } else { min_len as usize };
    // 同 mem_scan：提权失败留痕但不中断
    if !enable_debug_privilege() {
        tracing::warn!("SeDebugPrivilege not enabled: system-owned processes may be unreadable");
    }
    if pid != 0 {
        let msg = mem_scan_single(&request_id, pid, min_len, &keyword);
        let _ = tx.send(msg).await; // 上报通道已断（server 连接消失）：单进程结果帧丢失，由调用方超时兜底
        return;
    }
    mem_scan_all_inner(&request_id, min_len, &keyword, Some(&tx)).await;
}

fn mem_scan_single(request_id: &str, pid: i32, min_len: usize, keyword: &str) -> AgentMessage {
    match mem_scan_impl(pid, min_len, keyword) {
        Ok((matches, hits, scanned, truncated)) => AgentMessage {
            kind: Some(agent_message::Kind::MemScanResult(MemScanResult {
                request_id: request_id.to_string(),
                pid,
                matches,
                hits,
                scanned_bytes: scanned,
                truncated,
                regions_total: 0,
                regions_scanned: 0,
                timed_out: false,
                error: None,
                pids_total: 1,
                pids_scanned: 1,
                finished: true,
            })),
        },
        Err(e) => AgentMessage {
            kind: Some(agent_message::Kind::MemScanResult(MemScanResult {
                request_id: request_id.to_string(),
                pid,
                matches: vec![],
                hits: vec![],
                scanned_bytes: 0,
                truncated: false,
                regions_total: 0,
                regions_scanned: 0,
                timed_out: false,
                error: Some(e),
                pids_total: 1,
                pids_scanned: 0,
                finished: true,
            })),
        },
    }
}

fn mem_scan_impl(
    pid: i32,
    min_len: usize,
    keyword: &str,
) -> Result<(Vec<String>, Vec<MemMatch>, u64, bool), String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows_sys::Win32::System::Memory::VirtualQueryEx;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid as u32) };
    if handle.is_null() {
        return Err("OpenProcess 失败（无权限或进程不存在）".to_string());
    }

    tracing::debug!(pid, "mem_scan_impl: starting");
    let mut matches: Vec<String> = Vec::new();
    let mut hits: Vec<MemMatch> = Vec::new();
    let mut scanned: u64 = 0;
    let mut addr: usize = 0;
    let kw_lower = keyword.to_ascii_lowercase();

    unsafe {
        let mut mbi: windows_sys::Win32::System::Memory::MEMORY_BASIC_INFORMATION =
            std::mem::zeroed();
        let size =
            std::mem::size_of::<windows_sys::Win32::System::Memory::MEMORY_BASIC_INFORMATION>();
        loop {
            if VirtualQueryEx(handle, addr as *const _, &mut mbi, size) == 0 {
                break;
            }
            let region_size = mbi.RegionSize as usize;
            let committed = mbi.State == 0x1000;
            let readable =
                mbi.Protect & (0x02 | 0x04 | 0x20 | 0x40) != 0 && mbi.Protect & 0x100 == 0;
            if committed && readable && region_size > 0 && region_size <= 0x1000_0000 {
                let mut chunk = vec![0u8; region_size];
                let mut nread = 0usize;
                let ok = ReadProcessMemory(
                    handle,
                    mbi.BaseAddress as *const _,
                    chunk.as_mut_ptr().cast(),
                    region_size,
                    &mut nread,
                );
                if ok != 0 && nread > 0 {
                    scanned += nread as u64;
                    for s in extract_ascii_strings(&chunk[..nread], min_len)
                        .into_iter()
                        .chain(extract_utf16_strings(&chunk[..nread], min_len))
                    {
                        // 有关键词时 matches 只收命中（前端展示口径 = 过滤后）
                        let hit =
                            !kw_lower.is_empty() && s.to_ascii_lowercase().contains(&kw_lower);
                        if kw_lower.is_empty() {
                            if matches.len() < 5000 {
                                matches.push(s.clone());
                            }
                        } else if hit && matches.len() < 5000 {
                            matches.push(s.clone());
                        }
                        if !hit || kw_lower.is_empty() {
                            continue;
                        }
                        let addr_hex = format!("{:#x}", addr);
                        hits.push(MemMatch {
                            addr: addr_hex,
                            kind: "mem".into(),
                            value: truncate_utf8(&s, 200),
                        });
                        if hits.len() >= 5000 {
                            break;
                        }
                    }
                }
            }
            addr += mbi.RegionSize as usize;
            if addr == 0 {
                break;
            }
        }
    }

    unsafe { CloseHandle(handle) };
    tracing::debug!(
        pid,
        matches = matches.len(),
        hits = hits.len(),
        scanned,
        "mem_scan_impl: done"
    );
    let truncated = hits.len() >= 5000;
    Ok((matches, hits, scanned, truncated))
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn probe_process_enum() {
        use windows_sys::Win32::System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        };
        unsafe {
            let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
            println!("snapshot handle null: {}", snap.is_null());
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
            let first = Process32FirstW(snap, &mut entry);
            println!("Process32FirstW: {}", first);
            if first != 0 {
                let mut n = 0;
                loop {
                    n += 1;
                    if n <= 3 {
                        let end = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(260);
                        println!(
                            "  pid={} name={}",
                            entry.th32ProcessID,
                            String::from_utf16_lossy(&entry.szExeFile[..end])
                        );
                    }
                    if Process32NextW(snap, &mut entry) == 0 {
                        break;
                    }
                }
                println!("total processes: {n}");
            }
        }
    }
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[tokio::test]
    #[ignore]
    async fn probe_stream_frames() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<AgentMessage>(64);
        let handle = tokio::spawn(async move {
            mem_scan_stream("probe".to_string(), 0, 6, ".exe".to_string(), tx).await;
        });
        let mut n = 0;
        let mut with_content = 0;
        while let Some(msg) = rx.recv().await {
            if let Some(agent_message::Kind::MemScanResult(r)) = msg.kind {
                n += 1;
                if !r.matches.is_empty() {
                    with_content += 1;
                }
                if n <= 3 || r.finished {
                    println!(
                        "frame pid={} matches={} scanned={}MB fin={} err={:?}",
                        r.pid,
                        r.matches.len(),
                        r.scanned_bytes / 1048576,
                        r.finished,
                        r.error
                    );
                }
            }
            if n >= 60 {
                break;
            }
        }
        handle.abort();
        println!("frames={n} with_content={with_content}");
    }
}
