//! 文件系统操作：列目录（ls）。

use helm_proto::pb::{AgentMessage, FileEntry, FileListResult, agent_message};

/// 列目录，返回 FileListResult 消息。
pub fn list_dir(request_id: &str, path: &str) -> AgentMessage {
    let result = std::fs::read_dir(path).map(|rd| {
        let mut entries = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let meta = e.metadata().ok();
            let is_dir = meta.as_ref().is_some_and(|m| m.is_dir());
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let modified = meta
                .as_ref()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            entries.push(FileEntry {
                name,
                is_dir,
                size,
                modified_unix_ms: modified,
                mode: mode_string(&meta),
            });
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    });

    let fr = match result {
        Ok(entries) => FileListResult {
            request_id: request_id.to_string(),
            path: path.to_string(),
            entries,
            error: None,
        },
        Err(e) => FileListResult {
            request_id: request_id.to_string(),
            path: path.to_string(),
            entries: vec![],
            error: Some(e.to_string()),
        },
    };

    AgentMessage {
        kind: Some(agent_message::Kind::FileListResult(fr)),
    }
}

/// 权限字符串（Unix 八进制；Windows 空）。
fn mode_string(meta: &Option<std::fs::Metadata>) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.as_ref().map(|m| m.permissions().mode()).unwrap_or(0);
        format!("{:o}", mode & 0o777)
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        String::new()
    }
}
