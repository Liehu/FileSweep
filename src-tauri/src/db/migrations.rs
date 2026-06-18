use log::{info, warn};
use rusqlite::Connection;

pub fn migrate(db: &Connection) -> Result<(), String> {
    let migrations = [
        "CREATE TABLE IF NOT EXISTS file_records (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            version TEXT DEFAULT '',
            category TEXT DEFAULT '',
            local_path TEXT,
            file_size INTEGER NOT NULL,
            file_hash TEXT NOT NULL,
            extension TEXT DEFAULT '',
            functional_category TEXT DEFAULT '',
            status TEXT DEFAULT 'active',
            ai_skip INTEGER DEFAULT 0,
            scanned_at TEXT NOT NULL,
            mod_time TEXT DEFAULT '',
            catalog_id TEXT,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        "CREATE TABLE IF NOT EXISTS catalog_entries (
            id TEXT PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            description TEXT DEFAULT '',
            homepage_url TEXT DEFAULT '',
            download_url TEXT DEFAULT '',
            latest_version TEXT DEFAULT '',
            license TEXT DEFAULT '',
            functional_category TEXT DEFAULT '',
            tags TEXT DEFAULT '[]',
            ai_confidence REAL DEFAULT 0,
            ai_provider TEXT DEFAULT '',
            meta_updated_at TEXT,
            notes TEXT DEFAULT '',
            needs_review INTEGER DEFAULT 0,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        "CREATE TABLE IF NOT EXISTS operation_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp TEXT NOT NULL,
            operation TEXT NOT NULL,
            source_path TEXT NOT NULL,
            dest_path TEXT DEFAULT '',
            reason TEXT DEFAULT '',
            file_hash TEXT DEFAULT '',
            file_size INTEGER DEFAULT 0,
            status TEXT DEFAULT 'success',
            session_id TEXT DEFAULT '',
            can_revert INTEGER DEFAULT 0
        )",
        "CREATE INDEX IF NOT EXISTS idx_file_records_hash ON file_records(file_hash)",
        "CREATE INDEX IF NOT EXISTS idx_file_records_category ON file_records(category)",
        "CREATE INDEX IF NOT EXISTS idx_file_records_status ON file_records(status)",
        "CREATE INDEX IF NOT EXISTS idx_operation_logs_session ON operation_logs(session_id)",
        "CREATE INDEX IF NOT EXISTS idx_catalog_entries_name ON catalog_entries(name)",
        "CREATE TABLE IF NOT EXISTS categories (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            parent_id TEXT DEFAULT '',
            target_path TEXT DEFAULT '',
            extensions TEXT DEFAULT '[]',
            name_keywords TEXT DEFAULT '[]',
            sort_order INTEGER DEFAULT 0,
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
        "CREATE INDEX IF NOT EXISTS idx_categories_parent ON categories(parent_id)",
        "CREATE TABLE IF NOT EXISTS tags (
            id TEXT PRIMARY KEY,
            name TEXT UNIQUE NOT NULL,
            color TEXT DEFAULT '#185FA5',
            description TEXT DEFAULT '',
            created_at TEXT DEFAULT CURRENT_TIMESTAMP
        )",
    ];

    for sql in &migrations {
        db.execute_batch(sql)
            .map_err(|e| format!("执行迁移失败: {}", e))?;
    }

    // Schema patches
    let patches = [
        ("file_records", "mod_time", "TEXT DEFAULT ''"),
        ("file_records", "catalog_id", "TEXT"),
        ("file_records", "functional_category", "TEXT DEFAULT ''"),
        ("catalog_entries", "functional_category", "TEXT DEFAULT ''"),
        ("file_records", "is_app_dir", "INTEGER DEFAULT 0"),
        ("file_records", "app_dir_path", "TEXT DEFAULT ''"),
        ("file_records", "app_dir_reason", "TEXT DEFAULT ''"),
        ("catalog_entries", "ai_skip", "INTEGER DEFAULT 0"),
        ("file_records", "action", "TEXT DEFAULT ''"),
        ("file_records", "move_target", "TEXT DEFAULT ''"),
    ];

    for (table, column, def) in &patches {
        if !column_exists(db, table, column) {
            let sql = format!("ALTER TABLE {} ADD COLUMN {} {}", table, column, def);
            match db.execute_batch(&sql) {
                Ok(_) => {
                    info!("数据库补丁已应用: table={} column={}", table, column);
                }
                Err(e) => {
                    warn!("迁移补丁失败: sql={} error={}", sql, e);
                }
            }
        }
    }

    Ok(())
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("SELECT {} FROM {} LIMIT 0", column, table);
    conn.prepare(&sql).is_ok()
}
