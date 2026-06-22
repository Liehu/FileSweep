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
        // ── 配置 DB 化（P-config）──
        "CREATE TABLE IF NOT EXISTS software_roots (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            path TEXT NOT NULL UNIQUE,
            enabled INTEGER DEFAULT 1,
            display_name TEXT DEFAULT ''
        )",
        "CREATE TABLE IF NOT EXISTS category_rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            target_path TEXT DEFAULT '',
            extensions TEXT DEFAULT '[]',
            name_keywords TEXT DEFAULT '[]',
            app_dir_only INTEGER DEFAULT 0,
            priority INTEGER DEFAULT 0,
            enabled INTEGER DEFAULT 1
        )",
        "CREATE TABLE IF NOT EXISTS func_categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            keywords TEXT DEFAULT '[]',
            parent TEXT DEFAULT '',
            enabled INTEGER DEFAULT 1
        )",
        "CREATE TABLE IF NOT EXISTS exclude_rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_type TEXT NOT NULL,
            pattern TEXT NOT NULL,
            enabled INTEGER DEFAULT 1
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
        ("file_records", "app_executables", "TEXT DEFAULT '[]'"),
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

    // 初始化默认配置数据（仅在表为空时）
    init_default_config(db)?;

    Ok(())
}

/// 初始化默认配置（software_roots + 从 YAML 导入规则）
fn init_default_config(db: &Connection) -> Result<(), String> {
    // software_roots 默认路径（仅表为空时插入）
    let root_count: i64 = db
        .query_row("SELECT COUNT(*) FROM software_roots", [], |r| r.get(0))
        .unwrap_or(0);
    if root_count == 0 {
        let home = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let defaults = [
            ("C:\\Program Files", "C盘 Program Files"),
            ("C:\\Program Files (x86)", "C盘 Program Files (x86)"),
            ("D:\\Program Files", "D盘 Program Files"),
            ("E:\\Program Files", "E盘 Program Files"),
        ];
        for (path, name) in &defaults {
            let _ = db.execute(
                "INSERT OR IGNORE INTO software_roots (path, display_name) VALUES (?, ?)",
                rusqlite::params![path, name],
            );
        }
        if !home.is_empty() {
            let programs_path = format!("{}\\Programs", home);
            let _ = db.execute(
                "INSERT OR IGNORE INTO software_roots (path, display_name) VALUES (?, ?)",
                rusqlite::params![programs_path, "用户目录 Programs"],
            );
        }
        info!("已初始化默认软件安装根路径");
    }

    // category_rules：表为空时从 rules.yaml 导入或用默认
    let rule_count: i64 = db
        .query_row("SELECT COUNT(*) FROM category_rules", [], |r| r.get(0))
        .unwrap_or(0);
    if rule_count == 0 {
        import_rules_yaml(db);
    }

    // func_categories：表为空时从 categories.yaml 导入
    let cat_count: i64 = db
        .query_row("SELECT COUNT(*) FROM func_categories", [], |r| r.get(0))
        .unwrap_or(0);
    if cat_count == 0 {
        import_categories_yaml(db);
    }

    // exclude_rules 默认（表为空时）
    let exc_count: i64 = db
        .query_row("SELECT COUNT(*) FROM exclude_rules", [], |r| r.get(0))
        .unwrap_or(0);
    if exc_count == 0 {
        let defaults = [
            ("dir", "Windows"), ("dir", "$Recycle.Bin"), ("dir", "System Volume Information"),
            ("ext", ".tmp"), ("ext", ".log"),
            ("name", "Thumbs.db"), ("name", "desktop.ini"),
        ];
        for (rt, pat) in &defaults {
            let _ = db.execute(
                "INSERT INTO exclude_rules (rule_type, pattern) VALUES (?, ?)",
                rusqlite::params![rt, pat],
            );
        }
        info!("已初始化默认排除规则");
    }

    Ok(())
}

/// 从 rules.yaml 导入分类规则到 category_rules 表
fn import_rules_yaml(db: &Connection) {
    let yaml_paths = [
        "config/rules.yaml",
        "rules.yaml",
    ];
    for path in &yaml_paths {
        if let Ok(data) = std::fs::read_to_string(path) {
            if let Ok(cfg) = serde_yaml::from_str::<crate::core::models::RulesConfig>(&data) {
                for rule in &cfg.categories {
                    let exts = serde_json::to_string(&rule.extensions).unwrap_or_default();
                    let kws = serde_json::to_string(&rule.name_keywords).unwrap_or_default();
                    let _ = db.execute(
                        "INSERT INTO category_rules (name, target_path, extensions, name_keywords, app_dir_only, priority)
                         VALUES (?, ?, ?, ?, ?, ?)",
                        rusqlite::params![rule.name, rule.target_path, exts, kws,
                            if rule.app_dir_only { 1 } else { 0 }, 0],
                    );
                }
                info!("从 {} 导入了 {} 条分类规则", path, cfg.categories.len());
                return;
            }
        }
    }
    // YAML 不存在则用默认规则
    let defaults = crate::core::classifier::default_rules();
    for rule in &defaults.categories {
        let exts = serde_json::to_string(&rule.extensions).unwrap_or_default();
        let kws = serde_json::to_string(&rule.name_keywords).unwrap_or_default();
        let _ = db.execute(
            "INSERT INTO category_rules (name, target_path, extensions, name_keywords, app_dir_only)
             VALUES (?, ?, ?, ?, ?)",
            rusqlite::params![rule.name, rule.target_path, exts, kws,
                if rule.app_dir_only { 1 } else { 0 }],
        );
    }
    info!("使用默认分类规则");
}

/// 从 categories.yaml 导入功能分类到 func_categories 表
fn import_categories_yaml(db: &Connection) {
    let yaml_paths = [
        "config/categories.yaml",
        "categories.yaml",
    ];
    for path in &yaml_paths {
        if let Ok(data) = std::fs::read_to_string(path) {
            // categories.yaml 格式：categories: [{name, keywords}]
            #[derive(serde::Deserialize)]
            struct YamlCat {
                name: String,
                keywords: Vec<String>,
            }
            #[derive(serde::Deserialize)]
            struct YamlFile {
                categories: Vec<YamlCat>,
            }
            if let Ok(cfg) = serde_yaml::from_str::<YamlFile>(&data) {
                for cat in &cfg.categories {
                    let kws = serde_json::to_string(&cat.keywords).unwrap_or_default();
                    // parent 从 name 提取（如 "操作系统-引导管理" → "操作系统"）
                    let parent = cat.name.split('-').next().unwrap_or("").to_string();
                    let _ = db.execute(
                        "INSERT OR IGNORE INTO func_categories (name, keywords, parent) VALUES (?, ?, ?)",
                        rusqlite::params![cat.name, kws, parent],
                    );
                }
                info!("从 {} 导入了 {} 条功能分类", path, cfg.categories.len());
                return;
            }
        }
    }
    info!("无 categories.yaml 可导入，func_categories 表为空");
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("SELECT {} FROM {} LIMIT 0", column, table);
    conn.prepare(&sql).is_ok()
}
