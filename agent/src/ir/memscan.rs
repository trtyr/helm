//! 进程内存字符串扫描：VirtualQueryEx + ReadProcessMemory + ASCII/UTF-16LE 提取
//! （Volatility strings 简化版）。pid=0 时遍历全部进程（全局限额 + 超时），
//! 流式模式下逐进程推送部分批次。

use helm_proto::pb::{AgentMessage, MemMatch, MemScanResult, agent_message};

/// 全进程模式的全局限额（无截止时间，扫完为止；进度经流式实时可见）。
const ALL_MAX_MATCHES: usize = 5000;
const ALL_MAX_HITS: usize = 5000;
const PER_PROCESS_MATCH_CAP: usize = 100;

/// 启用 SeDebugPrivilege（管理员令牌默认持有但未激活——不显式启用则无法读 SYSTEM 进程内存）。
fn enable_debug_privilege() -> bool {
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }
        let name: Vec<u16> = "SeDebugPrivilege"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let mut luid = windows_sys::Win32::Foundation::LUID {
            LowPart: 0,
            HighPart: 0,
        };
        if LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return false;
        }
        let mut tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [windows_sys::Win32::Security::LUID_AND_ATTRIBUTES {
                Luid: windows_sys::Win32::Foundation::LUID {
                    LowPart: 0,
                    HighPart: 0,
                },
                Attributes: 0,
            }],
        };
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        let ok =
            AdjustTokenPrivileges(token, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut());
        CloseHandle(token);
        ok != 0
    }
}

pub async fn mem_scan(request_id: &str, pid: i32, min_len: u32, keyword: &str) -> AgentMessage {
    let min_len = if min_len == 0 { 6 } else { min_len as usize };
    let _ = enable_debug_privilege();
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
    let _ = enable_debug_privilege();
    if pid != 0 {
        let msg = mem_scan_single(&request_id, pid, min_len, &keyword);
        let _ = tx.send(msg).await;
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

/// 全进程模式：枚举进程逐一扫描，带全局命中限额与总超时。
/// partial = Some(tx) 时流式：每扫完一个进程推一帧部分批次（finished=false），结束帧 finished=true。
async fn mem_scan_all_inner(
    request_id: &str,
    min_len: usize,
    keyword: &str,
    partial: Option<&tokio::sync::mpsc::Sender<AgentMessage>>,
) -> AgentMessage {
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };

    let frame = |finished: bool,
                 pid: i32,
                 matches: Vec<String>,
                 hits: Vec<MemMatch>,
                 scanned: u64,
                 truncated: bool,
                 timed_out: bool,
                 total: u32,
                 scanned_n: u32| {
        AgentMessage {
            kind: Some(agent_message::Kind::MemScanResult(MemScanResult {
                request_id: request_id.to_string(),
                pid,
                matches,
                hits,
                scanned_bytes: scanned,
                truncated,
                regions_total: 0,
                regions_scanned: 0,
                timed_out,
                error: None,
                pids_total: total,
                pids_scanned: scanned_n,
                finished,
            })),
        }
    };

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() {
            let mut r = frame(true, 0, vec![], vec![], 0, false, false, 0, 0);
            if let Some(agent_message::Kind::MemScanResult(res)) = r.kind.as_mut() {
                res.error = Some("枚举进程失败（CreateToolhelp32Snapshot）".into());
            }
            return r;
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

        let total = procs.len() as u32;
        let mut all_matches: Vec<String> = Vec::new();
        let mut all_hits: Vec<MemMatch> = Vec::new();
        let mut scanned_bytes: u64 = 0;
        let mut scanned_n: u32 = 0;
        let mut truncated = false;
        let timed_out = false;

        for (pid, name) in procs {
            // 配额满后继续扫描统计覆盖，只是不再收集字符串——保证"进程 N/总数"是真实覆盖率
            let collect = all_matches.len() < ALL_MAX_MATCHES && all_hits.len() < ALL_MAX_HITS;
            let tag = format!("[{pid} {name}] ");
            tracing::debug!(pid, "mem_scan_all: scanning process");
            if let Ok((matches, hits, scanned, _)) = mem_scan_impl(pid as i32, min_len, keyword) {
                scanned_bytes += scanned;
                scanned_n += 1;
                let mut batch_matches: Vec<String> = Vec::new();
                let mut batch_hits: Vec<MemMatch> = Vec::new();
                if collect {
                    for s in matches.into_iter().take(PER_PROCESS_MATCH_CAP) {
                        if all_matches.len() + batch_matches.len() >= ALL_MAX_MATCHES {
                            truncated = true;
                            break;
                        }
                        batch_matches.push(format!("{}{}", tag, truncate_utf8(&s, 180)));
                    }
                    for h in hits.into_iter().take(PER_PROCESS_MATCH_CAP) {
                        if all_hits.len() + batch_hits.len() >= ALL_MAX_HITS {
                            truncated = true;
                            break;
                        }
                        batch_hits.push(MemMatch {
                            addr: format!("{}{}", tag, h.addr),
                            kind: h.kind,
                            value: h.value,
                        });
                    }
                    if all_matches.len() + batch_matches.len() >= ALL_MAX_MATCHES
                        || all_hits.len() + batch_hits.len() >= ALL_MAX_HITS
                    {
                        truncated = true;
                    }
                }
                all_matches.extend(batch_matches.iter().cloned());
                all_hits.extend(batch_hits.iter().cloned());
                if let Some(tx) = partial {
                    let msg = frame(
                        false,
                        pid as i32,
                        batch_matches,
                        batch_hits,
                        scanned_bytes,
                        truncated,
                        timed_out,
                        total,
                        scanned_n,
                    );
                    // 订阅端消失（WS 断开）则终止扫描
                    if tx.send(msg).await.is_err() {
                        break;
                    }
                }
            }
            // OpenProcess 失败（受保护/系统进程）直接跳过，不计失败
        }

        tracing::info!(
            pids_scanned = scanned_n,
            total,
            truncated,
            timed_out,
            "mem_scan_all: sweep done, sending final frame"
        );
        let final_msg = if partial.is_some() {
            frame(
                true,
                0,
                vec![],
                vec![],
                scanned_bytes,
                truncated,
                timed_out,
                total,
                scanned_n,
            )
        } else {
            frame(
                true,
                0,
                all_matches,
                all_hits,
                scanned_bytes,
                truncated,
                timed_out,
                total,
                scanned_n,
            )
        };
        if let Some(tx) = partial {
            match tx.send(final_msg).await {
                Ok(()) => tracing::info!("mem_scan_all: final frame sent"),
                Err(e) => tracing::warn!(error = %e, "mem_scan_all: final frame send FAILED"),
            }
            AgentMessage { kind: None }
        } else {
            final_msg
        }
    }
}

/// 提取 ASCII 可打印字符串（长度 ≥ min_len），按顺序返回。
pub fn extract_ascii_strings(buf: &[u8], min_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for (i, &b) in buf.iter().enumerate() {
        if (0x20..0x7F).contains(&b) {
            start.get_or_insert(i);
        } else if let Some(s) = start.take()
            && i - s >= min_len
        {
            out.push(String::from_utf8_lossy(&buf[s..i]).into_owned());
        }
    }
    if let Some(s) = start
        && buf.len() - s >= min_len
    {
        out.push(String::from_utf8_lossy(&buf[s..]).into_owned());
    }
    out
}

/// 提取 UTF-16LE 字符串（长度 ≥ min_len 字符）。
pub fn extract_utf16_strings(buf: &[u8], min_len: usize) -> Vec<String> {
    let mut out = Vec::new();
    let units: Vec<u16> = buf
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut start: Option<usize> = None;
    for (i, &u) in units.iter().enumerate() {
        let printable = (0x20..0x7F).contains(&u) || (0x4E00..=0x9FFF).contains(&u);
        if printable {
            start.get_or_insert(i);
        } else if let Some(s) = start.take()
            && i - s >= min_len
        {
            out.push(String::from_utf16_lossy(&units[s..i]));
        }
    }
    if let Some(s) = start
        && units.len() - s >= min_len
    {
        out.push(String::from_utf16_lossy(&units[s..]));
    }
    out
}

/// UTF-8 安全截断：不切断多字节字符。
fn truncate_utf8(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
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
