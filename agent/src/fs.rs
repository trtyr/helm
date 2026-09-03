//! 文件系统操作：列目录（ls）。

use helm_proto::pb::{AgentMessage, FileEntry, FileListResult, agent_message};

/// 列目录，返回 FileListResult 消息。
pub fn list_dir(request_id: &str, path: &str) -> AgentMessage {
    let result = std::fs::read_dir(path).map(|rd| {
        let mut entries = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            // follow symlink 判定（/tmp 等系统链接指向目录时按目录呈现）；
            // 断链 symlink 回退到链接自身的元数据。
            let meta = std::fs::metadata(e.path())
                .ok()
                .or_else(|| e.metadata().ok());
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

#[cfg(test)]
mod tests {
    use super::*;
    use helm_proto::pb::agent_message;

    fn entries_of(request_id: &str, path: &str) -> helm_proto::pb::FileListResult {
        match list_dir(request_id, path).kind {
            Some(agent_message::Kind::FileListResult(r)) => r,
            _ => panic!("expected FileListResult"),
        }
    }

    #[test]
    fn list_dir_returns_entries() {
        let dir = std::env::temp_dir().join(format!("helm-fs-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"x").unwrap();
        std::fs::create_dir_all(dir.join("sub")).unwrap();

        let r = entries_of("r1", dir.to_str().unwrap());
        assert!(r.error.is_none(), "unexpected error: {:?}", r.error);
        let names: Vec<&str> = r.entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"a.txt"));
        assert!(names.contains(&"sub"));
        let a = r.entries.iter().find(|e| e.name == "a.txt").unwrap();
        assert!(!a.is_dir);
        assert_eq!(a.size, 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_dir_symlink_to_dir_is_dir() {
        // 回归：/tmp 等指向目录的 symlink 应按目录呈现（follow 语义）
        let dir = std::env::temp_dir().join(format!("helm-fs-symlink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("real")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("real"), dir.join("link")).unwrap();

        let r = entries_of("r3", dir.to_str().unwrap());
        let link = r.entries.iter().find(|e| e.name == "link").unwrap();
        assert!(link.is_dir, "symlink → 目录应 is_dir=true");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_dir_missing_path_returns_error() {
        let r = entries_of("r2", "/nonexistent/helm-fs-test-xyz");
        assert!(r.error.is_some(), "expected error for missing path");
    }
}
