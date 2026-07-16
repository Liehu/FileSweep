//! 目录快照与增量 diff
//!
//! 扫描后保存目录快照（路径 → size + mtime + hash）。
//! 下次扫描同目录时，对每个文件比对 size + mtime——
//! 未变更的文件复用旧 hash，跳过哈希计算（增量扫描核心优化）。
//!
//! 快照存储为 JSON 文件，放在数据目录下 `<scan_dir_hash>.snapshot.json`。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 单个文件的快照条目（足以判断是否变更 + 复用 hash）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSnapshot {
    pub size: u64,
    /// 修改时间（UNIX 纳秒时间戳）
    pub mtime_nanos: u64,
    /// 上次扫描计算的 hash（复用）
    pub hash: String,
    /// 上次分类的 category（复用，避免重分类）
    pub category: String,
}

/// 目录快照：路径（相对扫描根）→ 文件条目
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DirSnapshot {
    /// 扫描根路径（绝对）
    pub root: String,
    /// 相对路径 → 快照条目
    pub files: HashMap<String, FileSnapshot>,
}

impl DirSnapshot {
    /// 快照文件存储路径：`<data_dir>/snapshots/<dir_hash>.snapshot.json`
    /// dir_hash = 扫描根路径的 blake3 前 16 字符，确保不同目录快照不冲突
    pub fn snapshot_path(data_dir: &Path, scan_root: &str) -> PathBuf {
        let mut hasher = blake3::Hasher::new();
        hasher.update(scan_root.as_bytes());
        let hash = hasher.finalize();
        let dir_hash = hex::encode(&hash.as_bytes()[..8]);
        let mut p = data_dir.to_path_buf();
        p.push("snapshots");
        p.push(format!("{}.snapshot.json", dir_hash));
        p
    }

    /// 保存快照到 JSON 文件
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建快照目录失败: {}", e))?;
        }
        let json = serde_json::to_string(self).map_err(|e| format!("序列化快照失败: {}", e))?;
        std::fs::write(path, json).map_err(|e| format!("写入快照失败: {}", e))
    }

    /// 从 JSON 文件加载快照。文件不存在返回 None（首次扫描）
    pub fn load(path: &Path) -> Option<DirSnapshot> {
        let data = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    }
}

/// 文件的当前元数据（用于和快照比对）
pub struct CurrentMeta {
    pub relative_path: String,
    pub size: u64,
    pub mtime_nanos: u64,
}

/// diff 结果：哪些文件可复用旧 hash，哪些需要重新计算
pub struct DiffResult {
    /// 可复用的文件（路径相对根 + 旧快照条目）
    pub reused: HashMap<String, FileSnapshot>,
    /// 需要重新扫描的文件（路径相对根 + 当前元数据）
    pub changed: Vec<CurrentMeta>,
}

/// 比对当前文件元数据与旧快照，返回复用/变更分类。
///
/// 判定规则：size + mtime 都相同 → 复用旧 hash；否则 → 需要重新扫描。
pub fn diff_snapshot(
    current_files: &[CurrentMeta],
    old: &DirSnapshot,
) -> DiffResult {
    let mut reused = HashMap::new();
    let mut changed = Vec::new();

    for f in current_files {
        match old.files.get(&f.relative_path) {
            Some(snap) if snap.size == f.size && snap.mtime_nanos == f.mtime_nanos => {
                // 未变更：复用旧 hash + category
                reused.insert(f.relative_path.clone(), snap.clone());
            }
            _ => {
                changed.push(CurrentMeta {
                    relative_path: f.relative_path.clone(),
                    size: f.size,
                    mtime_nanos: f.mtime_nanos,
                });
            }
        }
    }

    DiffResult { reused, changed }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diff_unchanged_reused() {
        let mut old = DirSnapshot::default();
        old.files.insert(
            "a.txt".into(),
            FileSnapshot {
                size: 100,
                mtime_nanos: 1000,
                hash: "b3:abc".into(),
                category: "文档".into(),
            },
        );
        let current = vec![CurrentMeta {
            relative_path: "a.txt".into(),
            size: 100,
            mtime_nanos: 1000,
        }];
        let diff = diff_snapshot(&current, &old);
        assert_eq!(diff.reused.len(), 1);
        assert!(diff.changed.is_empty());
        assert_eq!(diff.reused["a.txt"].hash, "b3:abc");
    }

    #[test]
    fn test_diff_size_changed() {
        let mut old = DirSnapshot::default();
        old.files.insert(
            "a.txt".into(),
            FileSnapshot {
                size: 100,
                mtime_nanos: 1000,
                hash: "b3:abc".into(),
                category: "文档".into(),
            },
        );
        // size 变了
        let current = vec![CurrentMeta {
            relative_path: "a.txt".into(),
            size: 200,
            mtime_nanos: 1000,
        }];
        let diff = diff_snapshot(&current, &old);
        assert!(diff.reused.is_empty());
        assert_eq!(diff.changed.len(), 1);
    }

    #[test]
    fn test_diff_new_file() {
        let old = DirSnapshot::default();
        let current = vec![CurrentMeta {
            relative_path: "new.txt".into(),
            size: 50,
            mtime_nanos: 2000,
        }];
        let diff = diff_snapshot(&current, &old);
        assert!(diff.reused.is_empty());
        assert_eq!(diff.changed.len(), 1);
    }

    #[test]
    fn test_snapshot_save_load_roundtrip() {
        let tmp = std::env::temp_dir().join(format!(
            "filesweep_snap_test_{}.json",
            uuid::Uuid::new_v4()
        ));
        let mut snap = DirSnapshot::default();
        snap.root = "/test".into();
        snap.files.insert(
            "a.txt".into(),
            FileSnapshot {
                size: 100,
                mtime_nanos: 1000,
                hash: "b3:abc".into(),
                category: "文档".into(),
            },
        );
        snap.save(&tmp).unwrap();
        let loaded = DirSnapshot::load(&tmp).expect("应能加载");
        assert_eq!(loaded.root, "/test");
        assert_eq!(loaded.files["a.txt"].hash, "b3:abc");
        let _ = std::fs::remove_file(&tmp);
    }
}
