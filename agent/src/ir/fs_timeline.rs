//! NTFS USN 文件活动时间线：FSCTL_READ_USN_JOURNAL 读变更日志（真实时间戳+原因码），
//! 按"最近 N 小时"过滤，应急取证回答"最近哪些文件被动过、动了什么"。

use helm_proto::pb::{AgentMessage, FsTimelineEntry, FsTimelineResult, agent_message};
// G11 拆分后各阶段函数各自持有 FFI 符号（原先集中在单函数的 `use` 块里）。
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Ioctl::{
    FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V1, USN_JOURNAL_DATA_V0,
};

const MAX_ENTRIES: usize = 20_000;

/// NTFS USN 时间线采集入口（阶段编排）。
///
/// **G11 拆分（2026-09-21）**：原为 193 行单块（盘符校验 + 卷打开 + 日志查询 + 逐记录解析 +
/// 排序截断全在一处）。现按阶段拆为具名函数——`validate_drive` / `normalize_limit` /
/// `open_volume` / `query_usn_journal` / `read_journal_entries` / `scan_batch` /
/// `entry_from_record` / `decode_record_name`；本函数只做编排与错误路径回包。
pub fn fs_timeline(
    request_id: &str,
    drive: &str,
    since_hours: u32,
    limit: u32,
    keyword: &str,
) -> AgentMessage {
    let drive_char = match validate_drive(drive) {
        Ok(c) => c,
        Err(msg) => return error_reply(request_id, drive, msg),
    };
    // 注：`since_hours` 不参与过滤——这是本函数的历史行为，本轮回填只做结构拆分，不改语义。
    let _since_hours = since_hours;
    let limit = normalize_limit(limit);
    let kw_lower = keyword.to_ascii_lowercase();

    // 阶段 1：打开卷（需管理员权限）
    let Some(handle) = open_volume(drive_char) else {
        return error_reply(
            request_id,
            drive,
            format!("打开卷 {drive_char}: 失败（需管理员权限）"),
        );
    };

    // 阶段 2：查日志元信息（FirstUsn / UsnJournalID）
    let Some(journal) = query_usn_journal(handle) else {
        unsafe { CloseHandle(handle) };
        return error_reply(
            request_id,
            drive,
            "查询 USN 日志失败（卷可能未启用日志）".into(),
        );
    };

    // 阶段 3：前向读日志（记录按时序排列），按高价值操作 + 关键字过滤
    let mut entries: Vec<FsTimelineEntry> = Vec::new();
    let mut stats = JournalStats::default();
    unsafe {
        read_journal_entries(handle, &journal, &kw_lower, &mut entries, &mut stats);
        CloseHandle(handle);
    }

    // 阶段 4：按时间倒序保留最新 limit 条
    entries.sort_by_key(|e| std::cmp::Reverse(e.ts_unix));
    if entries.len() > limit {
        entries.truncate(limit);
        stats.truncated = true;
    }
    reply(
        request_id,
        drive,
        entries,
        stats.total,
        stats.truncated,
        None,
    )
}

/// 本轮读取的累计统计。
#[derive(Default)]
struct JournalStats {
    /// 扫过的记录总数（含被高价值过滤丢弃的）
    total: u32,
    /// 是否因超出 [`MAX_ENTRIES`] 或 `limit` 而丢弃过条目
    truncated: bool,
}

/// 盘符校验：取首个字符并确认是 ASCII 字母。
fn validate_drive(drive: &str) -> Result<char, String> {
    let drive_char = drive.trim().chars().next().unwrap_or('C');
    if drive_char.is_ascii_alphabetic() {
        Ok(drive_char)
    } else {
        Err(format!("无效盘符: {drive}"))
    }
}

/// 条数上限归一：0 = 默认 5000；上限 [`MAX_ENTRIES`]。
fn normalize_limit(limit: u32) -> usize {
    if limit == 0 {
        5000
    } else {
        (limit as usize).min(MAX_ENTRIES)
    }
}

/// 统一回包形状（成功/失败共用），保证 `request_id`/`drive` 恒被回填。
fn reply(
    request_id: &str,
    drive: &str,
    entries: Vec<FsTimelineEntry>,
    total: u32,
    truncated: bool,
    error: Option<String>,
) -> AgentMessage {
    AgentMessage {
        kind: Some(agent_message::Kind::FsTimelineResult(FsTimelineResult {
            request_id: request_id.to_string(),
            drive: drive.to_string(),
            entries,
            total_scanned: total,
            truncated,
            error,
        })),
    }
}

/// 失败回包（空结果 + 错误文案）。
fn error_reply(request_id: &str, drive: &str, msg: String) -> AgentMessage {
    reply(request_id, drive, Vec::new(), 0, false, Some(msg))
}

/// 阶段 1：打开卷句柄；失败（通常是权限不足）返回 `None`。
fn open_volume(drive_char: char) -> Option<windows_sys::Win32::Foundation::HANDLE> {
    use windows_sys::Win32::Foundation::{GENERIC_READ, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    let volume: Vec<u16> = format!(r"\\.\{}:", drive_char)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            volume.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        )
    };
    (handle != INVALID_HANDLE_VALUE).then_some(handle)
}

/// 阶段 2：查询 USN 日志元信息（`FirstUsn` / `UsnJournalID`）；卷未启用日志时返回 `None`。
fn query_usn_journal(
    handle: windows_sys::Win32::Foundation::HANDLE,
) -> Option<USN_JOURNAL_DATA_V0> {
    use windows_sys::Win32::System::IO::DeviceIoControl;

    unsafe {
        let mut journal: USN_JOURNAL_DATA_V0 = std::mem::zeroed();
        let mut returned: u32 = 0;
        let ok = DeviceIoControl(
            handle,
            FSCTL_QUERY_USN_JOURNAL,
            std::ptr::null(),
            0,
            &mut journal as *mut _ as *mut _,
            std::mem::size_of::<USN_JOURNAL_DATA_V0>() as u32,
            &mut returned,
            std::ptr::null_mut(),
        );
        (ok != 0).then_some(journal)
    }
}

/// 阶段 3：前向读日志直到读完，逐批交给 [`scan_batch`]。
fn read_journal_entries(
    handle: windows_sys::Win32::Foundation::HANDLE,
    journal: &USN_JOURNAL_DATA_V0,
    kw_lower: &str,
    entries: &mut Vec<FsTimelineEntry>,
    stats: &mut JournalStats,
) {
    use windows_sys::Win32::System::IO::DeviceIoControl;

    let mut rj = READ_USN_JOURNAL_DATA_V1 {
        StartUsn: journal.FirstUsn,
        ReasonMask: 0xFFFFFFFF,
        ReturnOnlyOnClose: 0,
        Timeout: 0,
        BytesToWaitFor: 0,
        UsnJournalID: journal.UsnJournalID,
        MinMajorVersion: 2,
        MaxMajorVersion: 2,
    };
    let mut buf = vec![0u8; 1024 * 1024];

    loop {
        let mut got: u32 = 0;
        let ok = unsafe {
            DeviceIoControl(
                handle,
                FSCTL_READ_USN_JOURNAL,
                &rj as *const READ_USN_JOURNAL_DATA_V1 as *const _,
                std::mem::size_of::<READ_USN_JOURNAL_DATA_V1>() as u32,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut got,
                std::ptr::null_mut(),
            )
        };
        // ok == 0 即 ERROR_HANDLE_EOF：日志读完
        if ok == 0 || got < 8 {
            return;
        }
        rj.StartUsn = i64::from_le_bytes(buf[0..8].try_into().unwrap()); // 前进
        scan_batch(&buf[..got as usize], kw_lower, entries, stats);
        if got < buf.len() as u32 {
            return; // 日志读完
        }
    }
}

/// 扫描一批读出的缓冲区：首 8 字节是下一批起点，其后是变长记录。
fn scan_batch(
    buf: &[u8],
    kw_lower: &str,
    entries: &mut Vec<FsTimelineEntry>,
    stats: &mut JournalStats,
) {
    let mut off = 8usize;
    while off + 8 <= buf.len() {
        let rec = &buf[off..];
        let record_len = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
        if record_len == 0 || record_len > rec.len() {
            return;
        }
        stats.total += 1;
        if let Some(entry) = entry_from_record(rec, kw_lower) {
            entries.push(entry);
            if entries.len() > MAX_ENTRIES {
                // 保留最新：丢最旧的
                let drop = entries.len() - MAX_ENTRIES;
                entries.drain(0..drop);
                stats.truncated = true;
            }
        }
        off += record_len;
    }
}

/// 单条 USN 记录 → 时间线条目；非高价值操作或关键字不匹配返回 `None`。
fn entry_from_record(rec: &[u8], kw_lower: &str) -> Option<FsTimelineEntry> {
    // 只保留高价值操作（创建/删除/重命名/数据覆盖），丢弃纯 close 事件
    const HIGH_VALUE: u32 = 0x100 | 0x200 | 0x2000 | 0x1;
    let reason_raw = u32::from_le_bytes(rec[40..44].try_into().unwrap());
    if reason_raw & HIGH_VALUE == 0 {
        return None;
    }
    let name = decode_record_name(rec);
    if !kw_lower.is_empty() && !name.to_ascii_lowercase().contains(kw_lower) {
        return None;
    }
    let ts_unix = filetime_to_unix(u64::from_le_bytes(rec[32..40].try_into().unwrap()));
    Some(FsTimelineEntry {
        name,
        frn: u64::from_le_bytes(rec[8..16].try_into().unwrap()),
        parent_frn: u64::from_le_bytes(rec[16..24].try_into().unwrap()),
        ts_unix: ts_unix.max(0) as u64,
        reason: decode_reason(reason_raw),
    })
}

/// 记录中的文件名（UTF-16 解码）；偏移/长度越界或空长度返回空串。
fn decode_record_name(rec: &[u8]) -> String {
    let name_len = u16::from_le_bytes(rec[56..58].try_into().unwrap()) as usize;
    let name_off = u16::from_le_bytes(rec[58..60].try_into().unwrap()) as usize;
    if name_len == 0 || name_off + name_len > rec.len() {
        return String::new();
    }
    let u16s: Vec<u16> = rec[name_off..name_off + name_len]
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16_lossy(&u16s)
}

/// FILETIME（1601 起 100ns）→ unix 秒。
fn filetime_to_unix(ft: u64) -> i64 {
    (ft / 10_000_000) as i64 - 11_644_473_600
}

/// USN Reason 位掩码 → 短标签串。
fn decode_reason(reason: u32) -> String {
    const BITS: &[(&str, u32)] = &[
        ("file_create", 0x100),
        ("file_delete", 0x200),
        ("rename_new", 0x2000),
        ("rename_old", 0x1000),
        ("data_overwrite", 0x1),
        ("data_extend", 0x2),
        ("data_truncate", 0x4),
    ];
    let set: Vec<&str> = BITS
        .iter()
        .filter(|(_, bit)| reason & bit != 0)
        .map(|(n, _)| *n)
        .collect();
    if set.is_empty() {
        "close".into()
    } else {
        set.join("+")
    }
}

#[cfg(test)]
mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    #[ignore]
    fn probe_usn_raw() {
        use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::IO::DeviceIoControl;
        use windows_sys::Win32::System::Ioctl::{FSCTL_ENUM_USN_DATA, MFT_ENUM_DATA_V0};
        unsafe {
            let volume: Vec<u16> = r"\\.\D:".encode_utf16().chain(std::iter::once(0)).collect();
            let handle = CreateFileW(
                volume.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            );
            assert!(handle != INVALID_HANDLE_VALUE, "open volume fail");
            let mut med = MFT_ENUM_DATA_V0 {
                StartFileReferenceNumber: 0,
                LowUsn: 0,
                HighUsn: i64::MAX,
            };
            let mut buf = vec![0u8; 16 * 1024 * 1024];
            let mut printed = 0;
            for _round in 0..50 {
                let mut returned = 0u32;
                let med_bytes = std::slice::from_raw_parts(
                    &med as *const MFT_ENUM_DATA_V0 as *const u8,
                    std::mem::size_of::<MFT_ENUM_DATA_V0>(),
                );
                let ok = DeviceIoControl(
                    handle,
                    FSCTL_ENUM_USN_DATA,
                    med_bytes.as_ptr() as *const _,
                    med_bytes.len() as u32,
                    buf.as_mut_ptr() as *mut _,
                    buf.len() as u32,
                    &mut returned,
                    std::ptr::null_mut(),
                );
                if ok == 0 || returned < 8 {
                    println!("end: ok={ok} returned={returned}");
                    break;
                }
                let next = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                med.StartFileReferenceNumber = next;
                let mut off = 8;
                while off + 8 <= returned as usize {
                    let rec = &buf[off..returned as usize];
                    let len = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
                    if len == 0 || off + len > returned as usize {
                        break;
                    }
                    let ts = u64::from_le_bytes(rec[32..40].try_into().unwrap());
                    let nl = u16::from_le_bytes(rec[56..58].try_into().unwrap()) as usize;
                    let no = u16::from_le_bytes(rec[58..60].try_into().unwrap()) as usize;
                    if printed < 8 {
                        let name_raw = &rec[no..no + nl];
                        let u16s: Vec<u16> = name_raw
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        println!(
                            "rec: len={len} ts_ft={} ts_unix={} name={}",
                            ts,
                            (ts / 10_000_000) as i64 - 11_644_473_600,
                            String::from_utf16_lossy(&u16s)
                        );
                        printed += 1;
                    }
                    off += len;
                }
            }
            CloseHandle(handle);
        }
    }
}
