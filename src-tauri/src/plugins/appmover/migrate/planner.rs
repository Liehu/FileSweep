//! 迁移计划：解析目标根映射、空间预检、锁定检测（grill Q4）。

use std::path::PathBuf;

use rusqlite::Connection;

use crate::plugins::appmover::identify::dir_size_and_count;
use crate::plugins::appmover::migrate::locker::scan_locks;
use crate::plugins::appmover::models::{MigratePlan, TargetMap};

/// 根据目标根映射表，为 source_path 解析出 target_path。
/// 映射规则：source_path 必须以某 source_root 为前缀，target_path = target_root + 余下相对路径。
pub fn resolve_target(target_map: &[TargetMap], source_path: &str) -> Option<String> {
    let src_norm = source_path.replace('/', "\\");
    for m in target_map {
        let root = m.source_root.replace('/', "\\").trim_end_matches('\\').to_string();
        let root_pref = format!("{}\\", root.to_ascii_lowercase());
        if src_norm.to_ascii_lowercase().starts_with(&root_pref) {
            let rest = &src_norm[root.len()..]; // 含前导 '\'
            return Some(format!(
                "{}{}",
                m.target_root.trim_end_matches('\\'),
                rest
            ));
        }
        // source_path 恰好等于 source_root
        if src_norm.to_ascii_lowercase() == root.to_ascii_lowercase() {
            return Some(m.target_root.clone());
        }
    }
    None
}

/// 取全部目标根映射。
pub fn list_target_map(db: &Connection) -> rusqlite::Result<Vec<TargetMap>> {
    let mut stmt = db.prepare("SELECT source_root, target_root FROM am_target_map ORDER BY source_root")?;
    let rows = stmt.query_map([], |r| {
        Ok(TargetMap {
            source_root: r.get::<_, String>(0)?,
            target_root: r.get::<_, String>(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn set_target_map(db: &Connection, source_root: &str, target_root: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO am_target_map (source_root, target_root) VALUES (?1, ?2)
         ON CONFLICT(source_root) DO UPDATE SET target_root = excluded.target_root",
        rusqlite::params![source_root, target_root],
    )?;
    Ok(())
}

pub fn remove_target_map(db: &Connection, source_root: &str) -> rusqlite::Result<()> {
    db.execute(
        "DELETE FROM am_target_map WHERE source_root = ?1",
        rusqlite::params![source_root],
    )?;
    Ok(())
}

/// 构建迁移计划（执行前预演）。
/// 返回 Err 表示无法解析目标路径（用户未配置该根的映射）。
pub fn build_plan(db: &Connection, source_path: &str) -> Result<MigratePlan, String> {
    let map = list_target_map(db).map_err(|e| e.to_string())?;
    let target_path = resolve_target(&map, source_path)
        .ok_or_else(|| format!("未配置源根映射：{}", source_path))?;

    let src = PathBuf::from(source_path);
    let (size, file_count) = if src.is_dir() {
        dir_size_and_count(&src)
    } else {
        (0, 0)
    };

    // 空间预检：目标盘剩余 ≥ size * 1.1
    let target_free = disk_free(&target_path);
    let space_ok = target_free >= (size as f64 * 1.1) as u64;

    let locks = scan_locks(source_path);

    Ok(MigratePlan {
        source_path: source_path.to_string(),
        target_path,
        size_bytes: size,
        file_count,
        target_free_bytes: target_free,
        space_ok,
        locks,
    })
}

/// 获取指定路径所在盘的剩余空间（字节）。
fn disk_free(path: &str) -> u64 {
    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use winapi::um::fileapi::GetDiskFreeSpaceExW;
        use winapi::um::winnt::PULARGE_INTEGER;
        // 取盘符根：X:\
        let root = if path.len() >= 2 {
            let bytes: Vec<u16> = OsStr::new(&path[..2])
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            Some(bytes)
        } else {
            None
        };
        if let Some(wide) = root {
            unsafe {
                let mut free: u64 = 0;
                let _ok = GetDiskFreeSpaceExW(
                    wide.as_ptr(),
                    &mut free as *mut u64 as PULARGE_INTEGER,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
                return free;
            }
        }
        0
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE am_target_map (source_root TEXT PRIMARY KEY, target_root TEXT NOT NULL);")
            .unwrap();
        db
    }

    #[test]
    fn test_resolve_target() {
        let map = vec![TargetMap {
            source_root: "C:\\Users\\X\\AppData\\Roaming".into(),
            target_root: "D:\\Users\\X\\AppData\\Roaming".into(),
        }];
        let t = resolve_target(&map, "C:\\Users\\X\\AppData\\Roaming\\Tencent").unwrap();
        assert_eq!(t, "D:\\Users\\X\\AppData\\Roaming\\Tencent");
    }

    #[test]
    fn test_resolve_target_no_match() {
        let map = vec![];
        assert!(resolve_target(&map, "C:\\Foo").is_none());
    }

    #[test]
    fn test_set_and_list_target_map() {
        let db = mem_db();
        set_target_map(&db, "C:\\A", "D:\\A").unwrap();
        set_target_map(&db, "C:\\A", "E:\\A").unwrap(); // 覆盖
        let m = list_target_map(&db).unwrap();
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].target_root, "E:\\A");
    }
}
