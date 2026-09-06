//! Skill 包构建：把内嵌的 `skill/` 目录打包为 zip，并生成清单。
//!
//! 包随 Server 二进制发布（include_dir 编译期嵌入），`GET /api/v1/skill` 下载、
//! `GET /api/v1/skill/manifest` 校验版本（决策 011）。JWT / API key 均可获取
//! ——key 本身就是 skill 的取用凭据，否则「先有 key 还是先有 skill」会死锁。

use crate::domain::Result;
use include_dir::{Dir, DirEntry, include_dir};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Write as _;

/// 内嵌的 skill 包源（仓库 `skill/` 目录）。
pub static SKILL_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/../skill");

/// 包名与版本（随 Server 版本走）。
pub const SKILL_NAME: &str = "helm-skill";
pub const SKILL_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 收集内嵌目录下全部文件，返回（zip 内相对路径, 内容）。
fn collect_files() -> Vec<(String, &'static [u8])> {
    fn walk(dir: &Dir<'static>, root: &str, out: &mut Vec<(String, &'static [u8])>) {
        for entry in dir.entries() {
            match entry {
                DirEntry::Dir(d) => walk(d, root, out),
                DirEntry::File(f) => {
                    let rel = f
                        .path()
                        .strip_prefix(root)
                        .unwrap_or(f.path())
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((rel, f.contents()));
                }
            }
        }
    }
    let root = SKILL_DIR.path().to_string_lossy().to_string();
    let mut files = Vec::new();
    walk(&SKILL_DIR, &root, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// 打包为 zip（deflate 压缩；脚本文件带可执行位）。
pub fn build_zip() -> Result<Vec<u8>> {
    let buf = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(buf);
    let options = zip::write::SimpleFileOptions::default();
    for (path, content) in collect_files() {
        let is_script = path.starts_with("scripts/");
        let options = if is_script {
            options.unix_permissions(0o755)
        } else {
            options.unix_permissions(0o644)
        };
        zip.start_file(&path, options)
            .map_err(|e| crate::domain::Error::Internal(format!("zip start_file {path}: {e}")))?;
        zip.write_all(content)
            .map_err(|e| crate::domain::Error::Internal(format!("zip write {path}: {e}")))?;
    }
    let buf = zip
        .finish()
        .map_err(|e| crate::domain::Error::Internal(format!("zip finish: {e}")))?;
    Ok(buf.into_inner())
}

/// 单文件描述（清单用）。
fn file_entry(path: &str, content: &[u8]) -> Value {
    let mut hasher = Sha256::new();
    hasher.update(content);
    json!({
        "path": path,
        "size": content.len(),
        "sha256": hex::encode(hasher.finalize()),
    })
}

/// 清单：包名、版本、文件列表（路径/大小/sha256），与 zip 内容一致。
pub fn manifest() -> Value {
    let files: Vec<Value> = collect_files()
        .iter()
        .map(|(path, content)| file_entry(path, content))
        .collect();
    json!({
        "name": SKILL_NAME,
        "version": SKILL_VERSION,
        "file_count": files.len(),
        "files": files,
    })
}

/// 下载文件名（如 `helm-skill-0.1.0.zip`）。
pub fn zip_filename() -> String {
    format!("{SKILL_NAME}-{SKILL_VERSION}.zip")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<(String, &'static [u8])> {
        collect_files()
    }

    #[test]
    fn embedded_package_has_expected_layout() {
        let files = files();
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        assert!(paths.contains(&"SKILL.md"), "paths: {paths:?}");
        assert!(paths.contains(&"scripts/common.py"));
        assert!(paths.contains(&"scripts/exec.py"));
        assert!(paths.contains(&"scripts/ws.py"));
        assert!(paths.contains(&"references/api.md"));
        assert!(paths.iter().all(|p| !p.starts_with('/')), "相对路径");
    }

    #[test]
    fn zip_is_valid_and_matches_embedded_files() {
        let bytes = build_zip().expect("build zip");
        assert!(bytes.len() > 1000, "zip 过小: {}", bytes.len());
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).expect("open zip");
        for (path, content) in files() {
            let mut entry = archive
                .by_name(&path)
                .unwrap_or_else(|e| panic!("zip 缺 {path}: {e}"));
            let mut got = Vec::new();
            std::io::Read::read_to_end(&mut entry, &mut got).unwrap();
            assert_eq!(got, content, "zip 内 {path} 内容不一致");
        }
    }

    #[test]
    fn manifest_lists_every_file_with_sha256() {
        let m = manifest();
        assert_eq!(m["name"], SKILL_NAME);
        assert_eq!(m["version"], SKILL_VERSION);
        let listed = m["files"].as_array().expect("files array");
        let embedded = files();
        assert_eq!(listed.len(), embedded.len());

        // 逐文件核对 sha256
        let by_path: std::collections::HashMap<_, _> = embedded.into_iter().collect();
        for f in listed {
            let path = f["path"].as_str().unwrap();
            let content = by_path[path];
            let mut hasher = Sha256::new();
            hasher.update(content);
            assert_eq!(
                f["sha256"].as_str().unwrap(),
                hex::encode(hasher.finalize())
            );
            assert_eq!(f["size"].as_u64().unwrap(), content.len() as u64);
        }
    }

    #[test]
    fn manifest_is_deterministic() {
        assert_eq!(manifest().to_string(), manifest().to_string());
        assert_eq!(build_zip().unwrap(), build_zip().unwrap());
    }
}
