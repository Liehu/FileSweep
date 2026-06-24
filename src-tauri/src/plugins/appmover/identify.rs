//! 候选迁移目录识别
//!
//! 逻辑（grill Q2 = iii 叠加）：
//!   保护集 = 强白名单 ∪ 基线 ∪ 用户
//!   候选集 = 监控根下"非保护集"的一级子目录（仅目录，不收散文件）。
//!
//! 监控根（仅当前用户；ProgramData 不迁，见 Q11）：
//!   - %USERPROFILE%\AppData\Roaming
//!   - %USERPROFILE%\AppData\Local
//!   - %USERPROFILE%\AppData\LocalLow
//!   - C:\Program Files
//!   - C:\Program Files (x86)

use std::collections::HashSet;
use std::path::PathBuf;

use rusqlite::Connection;

use crate::plugins::appmover::baseline::{protected_name_set, with_conn};
use crate::plugins::appmover::models::CandidateDir;
use crate::app::context::Context;

/// 默认监控根（动态解析用户目录）。
pub fn default_watch_roots() -> Vec<String> {
    let mut roots = Vec::new();
    if let Some(home) = dirs::home_dir() {
        let appdata = home.join("AppData");
        for sub in ["Roaming", "Local", "LocalLow"] {
            let p = appdata.join(sub);
            roots.push(p.to_string_lossy().to_string());
        }
    }
    roots.push("C:\\Program Files".into());
    roots.push("C:\\Program Files (x86)".into());
    roots
}

/// 扫描所有监控根，返回候选迁移目录列表（已过滤保护集）。
pub fn scan_candidates(
    db: &Connection,
    roots: Option<&[String]>,
) -> Result<Vec<CandidateDir>, String> {
    let protected: HashSet<String> = protected_name_set(db).map_err(|e| e.to_string())?;
    let roots: Vec<String> = match roots {
        Some(r) => r.to_vec(),
        None => default_watch_roots(),
    };
    let mut out = Vec::new();
    for root in &roots {
        let root_path = PathBuf::from(root);
        if !root_path.is_dir() {
            continue;
        }
        let entries = match std::fs::read_dir(&root_path) {
            Ok(e) => e,
            Err(_) => continue, // 权限/不存在则跳过
        };
        for entry in entries.flatten() {
            let ft = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            // 只收目录，不收散文件（grill Q2-2c）
            if !ft.is_dir() && !is_junction(&entry.path()) {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            // 保护集（大小写不敏感比对目录名）
            if protected.contains(&name)
                || protected.iter().any(|p| p.eq_ignore_ascii_case(&name))
            {
                continue;
            }
            let path = entry.path();
            let is_jn = is_junction(&path);
            let (size, count) = if is_jn {
                // junction 不递归统计（避免穿越到 D: 重复计）
                (0u64, 0u64)
            } else {
                dir_size_and_count(&path)
            };
            out.push(CandidateDir {
                path: path.to_string_lossy().to_string(),
                name: name.clone(),
                watch_root: root.clone(),
                size_bytes: size,
                file_count: count,
                is_junction: is_jn,
                description: String::new(),
                software_name: String::new(),
            });
        }
    }
    Ok(out)
}

/// 包装：从 Context 执行扫描。
pub fn scan_candidates_ctx(ctx: &Context, roots: Option<&[String]>) -> Result<Vec<CandidateDir>, String> {
    with_conn(ctx, |conn| scan_candidates(conn, roots))
}

/// 递归统计目录总字节与文件数（跳过穿越点 / reparse point）。
pub fn dir_size_and_count(path: &std::path::Path) -> (u64, u64) {
    let mut size = 0u64;
    let mut count = 0u64;
    for entry in walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            size += entry.metadata().map(|m| m.len()).unwrap_or(0);
            count += 1;
        }
    }
    (size, count)
}

/// 判断路径是否为 junction / reparse point（已迁移过的标志）。
pub fn is_junction(path: &std::path::Path) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if let Ok(meta) = std::fs::symlink_metadata(path) {
            const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
            meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        } else {
            false
        }
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn mem_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE am_protected (path TEXT PRIMARY KEY, source TEXT DEFAULT 'user');",
        )
        .unwrap();
        db
    }

    #[test]
    fn test_scan_candidates_filters_protected() {
        let tmp = tempdir();
        let root = tmp.join("Roaming");
        fs::create_dir_all(root.join("Microsoft")).unwrap(); // 保护
        fs::create_dir_all(root.join("Packages")).unwrap(); // 保护
        fs::create_dir_all(root.join("Tencent")).unwrap(); // 候选
        fs::write(root.join("Tencent").join("a.txt"), b"hello").unwrap();

        let db = mem_db();
        crate::plugins::appmover::baseline::seed_hard_whitelist(&db).unwrap();

        let roots = vec![root.to_string_lossy().to_string()];
        let cands = scan_candidates(&db, Some(&roots)).unwrap();
        let names: Vec<_> = cands.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Tencent"), "候选应含 Tencent: {:?}", names);
        assert!(!names.contains(&"Microsoft"), "保护集应被过滤");
        assert!(!names.contains(&"Packages"));
        let tencent = cands.iter().find(|c| c.name == "Tencent").unwrap();
        assert_eq!(tencent.file_count, 1);
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("amtest_{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
