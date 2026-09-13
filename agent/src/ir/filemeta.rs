//! 文件元数据按需查询：SHA256 / 大小 / mtime（供 VirusTotal 查杀联动）。

use helm_proto::pb::{AgentMessage, FileMetaResult, agent_message};
use sha2::{Digest, Sha256};
use std::io::Read;

/// 计算并回包。
pub fn file_meta(request_id: &str, path: &str) -> AgentMessage {
    let reply = |sha256: String, size: u64, mtime: String, error: Option<String>| AgentMessage {
        kind: Some(agent_message::Kind::FileMetaResult(FileMetaResult {
            request_id: request_id.to_string(),
            path: path.to_string(),
            sha256,
            size,
            mtime,
            error,
        })),
    };

    match std::fs::metadata(path) {
        Ok(meta) => {
            let size = meta.len();
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs().to_string())
                .unwrap_or_default();
            match hash_file(path) {
                Ok(sha) => reply(sha, size, mtime, None),
                Err(e) => reply(String::new(), size, mtime, Some(e)),
            }
        }
        Err(e) => reply(
            String::new(),
            0,
            String::new(),
            Some(format!("文件不可读: {e}")),
        ),
    }
}

/// 流式 SHA256（上限 2GB 防滥用）。
fn hash_file(path: &str) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| format!("打开失败: {e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    let mut total: u64 = 0;
    loop {
        let n = f.read(&mut buf).map_err(|e| format!("读取失败: {e}"))?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > 2 * 1024 * 1024 * 1024 {
            return Err("文件超过 2GB，跳过哈希".into());
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}
