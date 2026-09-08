//! NTFS USN 文件活动时间线：FSCTL_READ_USN_JOURNAL 读变更日志（真实时间戳+原因码），
//! 按"最近 N 小时"过滤，应急取证回答"最近哪些文件被动过、动了什么"。

use helm_proto::pb::{AgentMessage, FsTimelineEntry, FsTimelineResult, agent_message};

const MAX_ENTRIES: usize = 20_000;

pub fn fs_timeline(
    request_id: &str,
    drive: &str,
    since_hours: u32,
    limit: u32,
    keyword: &str,
) -> AgentMessage {
    let reply = |entries: Vec<FsTimelineEntry>,
                 total: u32,
                 truncated: bool,
                 error: Option<String>,
                 drive: &str| {
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
    };

    let drive_char = drive.trim().chars().next().unwrap_or('C');
    if !drive_char.is_ascii_alphabetic() {
        return reply(vec![], 0, false, Some(format!("无效盘符: {drive}")), drive);
    }
    let _since_hours = since_hours;
    let limit = if limit == 0 { 5000 } else { (limit as usize).min(20_000) };
    let kw_lower = keyword.to_ascii_lowercase();

    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::IO::DeviceIoControl;
    use windows_sys::Win32::System::Ioctl::{
        FSCTL_QUERY_USN_JOURNAL, FSCTL_READ_USN_JOURNAL, READ_USN_JOURNAL_DATA_V1,
        USN_JOURNAL_DATA_V0,
    };

    let volume: Vec<u16> = format!(r"\\.\{}:", drive_char)
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let handle = CreateFileW(
            volume.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if handle == INVALID_HANDLE_VALUE {
            return reply(
                vec![],
                0,
                false,
                Some(format!("打开卷 {drive_char}: 失败（需管理员权限）")),
                drive,
            );
        }

        // 1) 查询日志元信息（FirstUsn / NextUsn）
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
        if ok == 0 {
            CloseHandle(handle);
            return reply(
                vec![],
                0,
                false,
                Some("查询 USN 日志失败（卷可能未启用日志）".into()),
                drive,
            );
        }

        // 2) 前向读日志（记录按时序排列），只保留时间窗口内的条目
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
        let mut entries: Vec<FsTimelineEntry> = Vec::new();
        let mut total: u32 = 0;
        let mut truncated = false;

        loop {
            let mut got: u32 = 0;
            let ok = DeviceIoControl(
                handle,
                FSCTL_READ_USN_JOURNAL,
                &rj as *const READ_USN_JOURNAL_DATA_V1 as *const _,
                std::mem::size_of::<READ_USN_JOURNAL_DATA_V1>() as u32,
                buf.as_mut_ptr() as *mut _,
                buf.len() as u32,
                &mut got,
                std::ptr::null_mut(),
            );
            if ok == 0 {
                break; // ERROR_HANDLE_EOF = 日志读完
            }
            if got < 8 {
                break;
            }
            rj.StartUsn = i64::from_le_bytes(buf[0..8].try_into().unwrap()); // 前进

            let mut off = 8usize;
            while off + 8 <= got as usize {
                let rec = &buf[off..got as usize];
                let record_len = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
                if record_len == 0 || off + record_len > got as usize {
                    break;
                }
                total += 1;
                let reason_raw = u32::from_le_bytes(rec[40..44].try_into().unwrap());
                // 只保留高价值操作（创建/删除/重命名/数据覆盖），丢弃纯 close 事件
                const HIGH_VALUE: u32 = 0x100 | 0x200 | 0x2000 | 0x1;
                if reason_raw & HIGH_VALUE == 0 {
                    off += record_len;
                    continue;
                }
                let ts_ft = u64::from_le_bytes(rec[32..40].try_into().unwrap());
                let ts_unix = filetime_to_unix(ts_ft);
                {
                    let name_len = u16::from_le_bytes(rec[56..58].try_into().unwrap()) as usize;
                    let name_off = u16::from_le_bytes(rec[58..60].try_into().unwrap()) as usize;
                    let name = if name_len > 0 && off + name_off + name_len <= got as usize {
                        let raw = &rec[name_off..name_off + name_len];
                        let u16s: Vec<u16> = raw
                            .chunks_exact(2)
                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                            .collect();
                        String::from_utf16_lossy(&u16s)
                    } else {
                        String::new()
                    };
                    if kw_lower.is_empty() || name.to_ascii_lowercase().contains(&kw_lower) {
                        entries.push(FsTimelineEntry {
                            name,
                            frn: u64::from_le_bytes(rec[8..16].try_into().unwrap()),
                            parent_frn: u64::from_le_bytes(rec[16..24].try_into().unwrap()),
                            ts_unix: ts_unix.max(0) as u64,
                            reason: decode_reason(reason_raw),
                        });
                        if entries.len() > MAX_ENTRIES {
                            // 保留最新：丢最旧的
                            let drop = entries.len() - MAX_ENTRIES;
                            entries.drain(0..drop);
                            truncated = true;
                        }
                    }
                }
                off += record_len;
            }
            if got < buf.len() as u32 {
                break; // 日志读完
            }
        }

        CloseHandle(handle);

        entries.sort_by_key(|e| std::cmp::Reverse(e.ts_unix));
        if entries.len() > limit {
            entries.truncate(limit);
            truncated = true;
        }
        reply(entries, total, truncated, None, drive)
    }
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
            let mut med = MFT_ENUM_DATA_V0 { StartFileReferenceNumber: 0, LowUsn: 0, HighUsn: i64::MAX };
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
                if ok == 0 || returned < 8 { println!("end: ok={ok} returned={returned}"); break; }
                let next = u64::from_le_bytes(buf[0..8].try_into().unwrap());
                med.StartFileReferenceNumber = next;
                let mut off = 8;
                while off + 8 <= returned as usize {
                    let rec = &buf[off..returned as usize];
                    let len = u32::from_le_bytes(rec[0..4].try_into().unwrap()) as usize;
                    if len == 0 || off + len > returned as usize { break; }
                    let ts = u64::from_le_bytes(rec[32..40].try_into().unwrap());
                    let nl = u16::from_le_bytes(rec[56..58].try_into().unwrap()) as usize;
                    let no = u16::from_le_bytes(rec[58..60].try_into().unwrap()) as usize;
                    if printed < 8 {
                        let name_raw = &rec[no..no + nl];
                        let u16s: Vec<u16> = name_raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                        println!("rec: len={len} ts_ft={} ts_unix={} name={}", ts, (ts / 10_000_000) as i64 - 11_644_473_600, String::from_utf16_lossy(&u16s));
                        printed += 1;
                    }
                    off += len;
                }
            }
            CloseHandle(handle);
        }
    }
}
