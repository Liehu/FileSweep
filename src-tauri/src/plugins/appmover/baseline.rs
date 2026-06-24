//! 基线与保护集管理
//!
//! 保护集 = 强白名单 ∪ 纯净VM基线 ∪ 用户手动加入。
//! 基线**仅用于**"识别系统默认目录"，不做快照恢复（见 grill 结论 Q3）。
//! 基线文件格式：每行一个相对或绝对路径（# 开头注释）。

use rusqlite::Connection;

use crate::app::context::Context;
use crate::plugins::appmover::models::ProtectedEntry;

/// 强白名单：无论在不在基线里都永远不碰的目录名（一级）。
/// 这些是 Windows / 通用系统组件的标准目录名。
pub const HARD_WHITELIST: &[&str] = &[
    "Microsoft",
    "Packages",
    "Windows",
    "WindowsApps",
    "Common Files",
    "Internet Explorer",
    "Windows Defender",
    "Windows Mail",
    "Windows Media Player",
    "Windows NT",
    "Windows Photo Viewer",
    "Windows Portable Devices",
    "Windows Security",
    "Windows Sidebar",
    "Microsoft.NET",
    "ModifiableWindowsApps",
    "Application Data",
    "Local Settings",
    "SendTo",
    "Start Menu",
    "Templates",
    "Cookies",
    "NetHood",
    "PrintHood",
    "Recent",
    "Desktop",
    "Documents",
    "Downloads",
    "Music",
    "Pictures",
    "Videos",
    "OneDrive",
    "Contacts",
    "Favorites",
    "Links",
    "Saved Games",
    "Searches",
    "3D Objects",
];

/// 写入硬白名单到 DB（仅首次 / source='hardcoded'）。
/// 重复调用幂等（INSERT OR IGNORE）。
pub fn seed_hard_whitelist(db: &Connection) -> rusqlite::Result<()> {
    for name in HARD_WHITELIST {
        db.execute(
            "INSERT OR IGNORE INTO am_protected (path, source) VALUES (?1, 'hardcoded')",
            rusqlite::params![*name],
        )?;
    }
    Ok(())
}

/// 从基线文件导入（每行一个路径/目录名）。
/// 文件可为绝对路径或相对名，统一按"basename"归一化存入保护集（source='baseline'）。
pub fn import_baseline_file(db: &Connection, file_path: &str) -> Result<usize, String> {
    let content =
        std::fs::read_to_string(file_path).map_err(|e| format!("读取基线文件失败: {}", e))?;
    let mut count = 0usize;
    for raw in content.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let name = basename(line);
        if name.is_empty() {
            continue;
        }
        db.execute(
            "INSERT OR IGNORE INTO am_protected (path, source) VALUES (?1, 'baseline')",
            rusqlite::params![name],
        )
        .map_err(|e| format!("写入保护集失败: {}", e))?;
        count += 1;
    }
    Ok(count)
}

/// 把一组扫描到的一级目录名作为基线写入（"首次扫描作基线"）。
pub fn import_baseline_names(db: &Connection, names: &[String]) -> Result<usize, String> {
    let mut count = 0usize;
    for name in names {
        if name.trim().is_empty() {
            continue;
        }
        db.execute(
            "INSERT OR IGNORE INTO am_protected (path, source) VALUES (?1, 'baseline')",
            rusqlite::params![name],
        )
        .map_err(|e| format!("写入保护集失败: {}", e))?;
        count += 1;
    }
    Ok(count)
}

/// 取保护集（目录名集合）。
pub fn list_protected(db: &Connection) -> rusqlite::Result<Vec<ProtectedEntry>> {
    let mut stmt = db.prepare("SELECT path, source FROM am_protected ORDER BY source, path")?;
    let rows = stmt.query_map([], |r| {
        Ok(ProtectedEntry {
            path: r.get::<_, String>(0)?,
            source: r.get::<_, String>(1)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 仅返回保护集的目录名（用于识别时快速比对）。
pub fn protected_name_set(db: &Connection) -> rusqlite::Result<std::collections::HashSet<String>> {
    let mut set = std::collections::HashSet::new();
    let mut stmt = db.prepare("SELECT path FROM am_protected")?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    for r in rows {
        set.insert(r?);
    }
    Ok(set)
}

pub fn add_protected(db: &Connection, name: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO am_protected (path, source) VALUES (?1, 'user')",
        rusqlite::params![name],
    )?;
    Ok(())
}

pub fn remove_protected(db: &Connection, name: &str) -> rusqlite::Result<()> {
    // 硬白名单不允许删除
    db.execute(
        "DELETE FROM am_protected WHERE path = ?1 AND source != 'hardcoded'",
        rusqlite::params![name],
    )?;
    Ok(())
}

/// 从 Context 拿到 conn 并执行闭包（spawn_blocking 友好）。
pub fn with_conn<F, T>(ctx: &Context, f: F) -> Result<T, String>
where
    F: FnOnce(&Connection) -> Result<T, String>,
{
    let db = ctx.db.clone();
    let conn = db.conn.lock().map_err(|e| format!("db lock: {}", e))?;
    // 首次确保硬白名单已种子
    seed_hard_whitelist(&conn).map_err(|e| format!("seed hard whitelist: {}", e))?;
    f(&conn)
}

/// 取路径的 basename（最后一段），处理 \ 和 /。
fn basename(p: &str) -> String {
    let p = p.trim_end_matches(['\\', '/']);
    match p.rsplit(['\\', '/']).next() {
        Some(s) => s.to_string(),
        None => p.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basename() {
        assert_eq!(basename("C:\\Users\\X\\AppData\\Roaming\\Tencent"), "Tencent");
        assert_eq!(basename("Tencent"), "Tencent");
        assert_eq!(basename("C:/Users/X/Microsoft/"), "Microsoft");
    }

    #[test]
    fn test_protected_set() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE am_protected (path TEXT PRIMARY KEY, source TEXT DEFAULT 'user');",
        )
        .unwrap();
        seed_hard_whitelist(&db).unwrap();
        let set = protected_name_set(&db).unwrap();
        assert!(set.contains("Microsoft"));
        assert!(set.contains("Packages"));
        // 删除硬白名单应无效
        remove_protected(&db, "Microsoft").unwrap();
        let set2 = protected_name_set(&db).unwrap();
        assert!(set2.contains("Microsoft"), "硬白名单不可删");
    }

    #[test]
    fn test_import_baseline_names() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE am_protected (path TEXT PRIMARY KEY, source TEXT DEFAULT 'user');",
        )
        .unwrap();
        let _ = import_baseline_names(&db, &["Foo".into(), "Bar".into()]).unwrap();
        let _ = import_baseline_names(&db, &["Foo".into()]).unwrap(); // 重复应幂等
        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM am_protected", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2, "重复导入应幂等，总数仍为 2");
    }
}
