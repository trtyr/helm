//! 文件系统操作：列目录（ls）+ Windows 驱动器根视图。

use helm_proto::pb::{AgentMessage, FileEntry, FileListResult, agent_message};

/// 列目录，返回 FileListResult 消息。
///
/// Windows 下空路径 / "/" / "\\" 视为「此电脑」根视图——枚举全部逻辑驱动器
/// （GetLogicalDrivesW，类型标注在 mode 字段）；此后跳转均使用 `C:\...` 原生路径。
pub fn list_dir(request_id: &str, path: &str) -> AgentMessage {
    #[cfg(windows)]
    if is_drive_root(path) {
        return drive_root_result(request_id, path);
    }

    // Windows 下统一正斜杠为原生反斜杠（前端可能传来混用形态）
    #[cfg(windows)]
    let normalized = path.replace('/', "\\");
    #[cfg(not(windows))]
    let normalized = if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    };

    let result = std::fs::read_dir(&normalized).map(|rd| {
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
        // 目录在前，再按名称排序
        entries.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        entries
    });

    let fr = match result {
        Ok(entries) => FileListResult {
            request_id: request_id.to_string(),
            path: normalized,
            entries,
            error: None,
        },
        Err(e) => FileListResult {
            request_id: request_id.to_string(),
            path: normalized,
            entries: vec![],
            error: Some(e.to_string()),
        },
    };

    AgentMessage {
        kind: Some(agent_message::Kind::FileListResult(fr)),
    }
}

/// 是否为「此电脑」根视图请求（空 / / / \\）。
#[cfg(windows)]
fn is_drive_root(path: &str) -> bool {
    let t = path.trim();
    t.is_empty() || t == "/" || t == "\\"
}

/// Windows 根视图：枚举逻辑驱动器（类型标注于 mode）。
#[cfg(windows)]
fn drive_root_result(request_id: &str, path: &str) -> AgentMessage {
    let entries = crate::win_native::list_drives()
        .into_iter()
        .map(|(name, kind)| FileEntry {
            name,
            is_dir: true,
            size: 0,
            modified_unix_ms: 0,
            mode: kind.to_string(),
        })
        .collect();
    AgentMessage {
        kind: Some(agent_message::Kind::FileListResult(FileListResult {
            request_id: request_id.to_string(),
            path: path.to_string(),
            entries,
            error: None,
        })),
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
        // Windows 建目录 symlink 需要管理员/开发者模式，无权限时跳过断言
        #[cfg(windows)]
        if std::os::windows::fs::symlink_dir(dir.join("real"), dir.join("link")).is_err() {
            let _ = std::fs::remove_dir_all(&dir);
            return;
        }

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
