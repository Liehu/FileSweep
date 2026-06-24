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
            app_dir_only INTEGER DEFAULT 0,
            priority INTEGER DEFAULT 0,
            enabled INTEGER DEFAULT 1
        )",
        "CREATE TABLE IF NOT EXISTS func_categories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            keywords TEXT DEFAULT '[]',
            parent TEXT DEFAULT '',
            description TEXT DEFAULT '',
            target_path TEXT DEFAULT '',
            enabled INTEGER DEFAULT 1
        )",
        "CREATE TABLE IF NOT EXISTS exclude_rules (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            rule_type TEXT NOT NULL,
            pattern TEXT NOT NULL,
            enabled INTEGER DEFAULT 1
        )",
        // ── AppMover 插件表 ──
        "CREATE TABLE IF NOT EXISTS am_target_map (
            source_root TEXT PRIMARY KEY,
            target_root TEXT NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS am_protected (
            path TEXT PRIMARY KEY,
            source TEXT DEFAULT 'user'
        )",
        "CREATE TABLE IF NOT EXISTS am_migrate_jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_path TEXT NOT NULL,
            target_path TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'planned',
            checkpoint TEXT DEFAULT '[]',
            file_count INTEGER DEFAULT 0,
            copied_count INTEGER DEFAULT 0,
            total_bytes INTEGER DEFAULT 0,
            started_at INTEGER,
            finished_at INTEGER,
            error TEXT DEFAULT ''
        )",
        "CREATE INDEX IF NOT EXISTS idx_am_migrate_jobs_status ON am_migrate_jobs(status)",
        "CREATE TABLE IF NOT EXISTS am_monitor_snapshot (
            watch_root TEXT NOT NULL,
            dir_name TEXT NOT NULL,
            first_seen_at INTEGER,
            last_seen_at INTEGER,
            state TEXT DEFAULT 'normal',
            PRIMARY KEY (watch_root, dir_name)
        )",
        "CREATE TABLE IF NOT EXISTS am_env_backup (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scope TEXT NOT NULL,
            key TEXT NOT NULL,
            value TEXT,
            backed_up_at INTEGER NOT NULL
        )",
        "CREATE TABLE IF NOT EXISTS am_describe_map (
            dir_name TEXT PRIMARY KEY,
            software_name TEXT NOT NULL,
            description TEXT DEFAULT '',
            source TEXT DEFAULT 'preset'
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
        ("func_categories", "description", "TEXT DEFAULT ''"),
        ("func_categories", "target_path", "TEXT DEFAULT ''"),
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

    // func_categories：表为空时优先从 CSV 种子数据录入，回退 categories.yaml
    let cat_count: i64 = db
        .query_row("SELECT COUNT(*) FROM func_categories", [], |r| r.get(0))
        .unwrap_or(0);
    if cat_count == 0 {
        seed_func_categories(db);
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
                    let _ = db.execute(
                        "INSERT INTO category_rules (name, target_path, extensions, app_dir_only, priority)
                         VALUES (?, ?, ?, ?, ?)",
                        rusqlite::params![rule.name, rule.target_path, exts,
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
        let _ = db.execute(
            "INSERT INTO category_rules (name, target_path, extensions, app_dir_only)
             VALUES (?, ?, ?, ?)",
            rusqlite::params![rule.name, rule.target_path, exts,
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

/// 从嵌入的 CSV 种子数据录入功能分类。
///
/// CSV 列：中文名称,一级分类,二级分类,三级分类,简写,描述
///
/// 映射规则（每行 CSV → 1 条 func_categories 记录）：
/// - `name`        = 简写列（如 Boot / Exp-Frameworks），冲突时追加 parent 前缀
/// - `parent`      = 一级分类的中文名（如 操作系统 / 知识库），通过 L1_CN_MAP 映射
/// - `keywords`    = 从描述提取的关键词（extract_keywords），用于指导 AI 分类
/// - `target_path` = 一级\二级\三级 英文层级（如 Security\Exploit\Frameworks），相对路径
/// - `description` = 描述列原文
const FUNC_CATEGORIES_CSV: &str = include_str!("../../软件分类.csv");

/// 一级分类英文 → 中文名映射（从 CSV 数据归纳）
fn l1_to_cn(l1: &str) -> &str {
    match l1 {
        "OS" => "操作系统",
        "EDA" => "电子设计",
        "Media" => "媒体管理",
        "SysEnhance" => "系统增强",
        "Dev" => "编程开发",
        "Office" => "学习办公",
        "Game" => "游戏娱乐",
        "Web" => "综合网站",
        "WebTool" => "在线工具",
        "Security" => "网络安全",
        "Wiki" => "知识库",
        _ => l1,
    }
}

/// 从描述文本提取关键词（简单规则，用于指导 AI 分类）。
///
/// 规则：
/// 1. 按中文标点和分隔词（如、及、等、包括）切分
/// 2. 过滤停用词（工具/软件/资源/包含/各类 等通用词）
/// 3. 保留长度≥2 的中文片段 + 英文专有名词（含字母的片段）
fn extract_keywords(desc: &str) -> Vec<String> {
    if desc.is_empty() {
        return Vec::new();
    }
    // 按常见分隔符切分：中文标点、如/包括/及/等/和/与/或/及各类
    let separators = ['、', '，', '。', '；', '：', '/', '，', ' '];
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in desc.chars() {
        if separators.contains(&ch)
            || (current.ends_with("如") && ch.is_ascii_alphabetic())
            || (current.ends_with("包括") && !current.ends_with("包"))
        {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
            // "如"/"包括" 作为分隔词，不保留
            if ch.is_ascii_alphabetic() {
                current.push(ch); // 英文开头继续累积
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    // 进一步按"如""包括""及""等"切分（这些词常出现在描述中间）
    let split_words = ["如", "包括", "及", "等", "和", "与", "以及"];
    let mut refined: Vec<String> = Vec::new();
    for token in &tokens {
        let mut parts = vec![token.as_str()];
        for word in &split_words {
            let mut new_parts = Vec::new();
            for part in &parts {
                for p in part.split(word) {
                    new_parts.push(p);
                }
            }
            parts = new_parts;
        }
        for p in parts {
            let p = p.trim();
            if !p.is_empty() {
                refined.push(p.to_string());
            }
        }
    }

    // 过滤停用词 + 太短的片段
    let stopwords = [
        "工具", "软件", "资源", "包含", "各类", "各种", "相关", "其他", "及", "等",
        "如", "包括", "和", "与", "或", "的", "等", "配置", "脚本", "工具包",
    ];
    let mut keywords: Vec<String> = Vec::new();
    for raw in &refined {
        let r = raw.trim();
        if r.len() < 2 {
            continue;
        }
        // 英文专有名词（含字母且不太长）直接保留
        let has_alpha = r.chars().any(|c| c.is_ascii_alphabetic());
        if has_alpha && r.len() <= 30 {
            if !keywords.iter().any(|k| k == r) {
                keywords.push(r.to_string());
            }
            continue;
        }
        // 中文片段：过滤停用词后缀/前缀（字符级操作，避免多字节切片 panic）
        let cleaned_raw = r.to_string();
        let mut chars: Vec<char> = cleaned_raw.chars().collect();
        let mut changed = true;
        while changed {
            changed = false;
            let s: String = chars.iter().collect();
            for sw in &stopwords {
                let sw_chars: Vec<char> = sw.chars().collect();
                if chars.len() > sw_chars.len() && s.ends_with(sw) {
                    chars.truncate(chars.len() - sw_chars.len());
                    changed = true;
                    break;
                }
                if chars.len() > sw_chars.len() && s.starts_with(sw) {
                    chars.drain(..sw_chars.len());
                    changed = true;
                    break;
                }
                if chars == sw_chars {
                    chars.clear();
                    changed = true;
                    break;
                }
            }
        }
        let cleaned: String = chars.iter().collect();
        if cleaned.chars().count() >= 2 && !keywords.iter().any(|k| k == &cleaned) {
            keywords.push(cleaned);
        }
    }
    // 限制最多 8 个关键词
    keywords.truncate(8);
    keywords
}

fn seed_func_categories(db: &Connection) {
    let mut count = 0;
    for (i, line) in FUNC_CATEGORIES_CSV.lines().enumerate() {
        if i == 0 {
            continue; // 跳过表头
        }
        if line.trim().is_empty() {
            continue;
        }
        let cols: Vec<&str> = line.splitn(6, ',').collect();
        if cols.len() < 5 {
            continue;
        }
        let cn_name = cols[0].trim();
        let l1 = cols[1].trim();
        let l2 = cols[2].trim();
        let l3 = cols[3].trim();
        let abbr = cols[4].trim();
        let desc = if cols.len() >= 6 { cols[5].trim() } else { "" };

        // name = 简写
        let name = abbr;

        // parent = 一级分类中文名
        let parent = l1_to_cn(l1).to_string();

        // target_path = 一级\二级\三级 英文层级（相对路径）
        let levels: Vec<&str> = [l1, l2, l3].iter().filter(|s| !s.is_empty()).copied().collect();
        let target_path = levels.join("\\");

        // keywords = 从描述提取 + 中文名称（确保至少有中文名兜底）
        let mut keywords = extract_keywords(desc);
        if !keywords.iter().any(|k| k == cn_name) {
            keywords.insert(0, cn_name.to_string());
        }
        let keywords_json = serde_json::to_string(&keywords).unwrap_or_else(|_| "[]".into());

        // name 唯一约束：简写冲突时用 parent\简写 重试
        let res = db.execute(
            "INSERT OR IGNORE INTO func_categories (name, keywords, parent, description, target_path)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![name, keywords_json, parent, desc, target_path],
        );
        let inserted = match res {
            Ok(n) => n,
            Err(_) => 0,
        };
        if inserted == 0 {
            let dedup_name = format!("{}\\{}", parent, abbr);
            let _ = db.execute(
                "INSERT OR IGNORE INTO func_categories (name, keywords, parent, description, target_path)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![dedup_name, keywords_json, parent, desc, target_path],
            );
        }
        count += 1;
    }
    if count > 0 {
        info!("从种子 CSV 录入了 {} 条功能分类", count);
    } else {
        warn!("种子 CSV 录入 0 条，回退到 categories.yaml");
        import_categories_yaml(db);
    }
}

fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    let sql = format!("SELECT {} FROM {} LIMIT 0", column, table);
    conn.prepare(&sql).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证嵌入的 CSV 种子数据格式正确：219 条数据行 + 表头校验
    #[test]
    fn test_seed_csv_parse() {
        let lines: Vec<&str> = FUNC_CATEGORIES_CSV.lines().collect();
        assert!(lines.len() > 1, "CSV 应含表头 + 数据行");

        let header_cols: Vec<&str> = lines[0].split(',').collect();
        assert_eq!(header_cols[0].trim(), "中文名称");
        assert_eq!(header_cols[4].trim(), "简写");

        let mut data_count = 0;
        for (i, line) in lines.iter().enumerate() {
            if i == 0 || line.trim().is_empty() {
                continue;
            }
            let cols: Vec<&str> = line.splitn(6, ',').collect();
            assert!(cols.len() >= 5, "第 {} 行列数不足: {}", i + 1, line);
            data_count += 1;
        }
        assert_eq!(data_count, 219, "应有 219 条数据行，实际 {}", data_count);
    }

    /// 验证 seed_func_categories 能把 219 条数据写入内存 DB，且字段映射正确
    #[test]
    fn test_seed_func_categories_inserts() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE func_categories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                keywords TEXT DEFAULT '[]',
                parent TEXT DEFAULT '',
                description TEXT DEFAULT '',
                target_path TEXT DEFAULT '',
                enabled INTEGER DEFAULT 1
            );",
        )
        .unwrap();

        seed_func_categories(&db);

        let count: i64 = db
            .query_row("SELECT COUNT(*) FROM func_categories", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 219, "应录入 219 条，实际 {}", count);

        // 行2「操作系统,OS,Boot,,Boot,系统引导管理工具如EasyBCD、rEFInd、GRUB配置等」
        // → name=Boot, parent=操作系统(中文), target_path=OS\Boot(英文层级)
        let row: (String, String, String, String) = db
            .query_row(
                "SELECT name, parent, target_path, description FROM func_categories WHERE name = 'Boot'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .expect("应存在 name=Boot 的记录");
        assert_eq!(row.0, "Boot");
        assert_eq!(row.1, "操作系统", "parent 应为中文一级分类名");
        assert_eq!(row.2, "OS\\Boot", "target_path 应为英文层级");
        assert!(row.3.contains("EasyBCD"), "description 应含 EasyBCD");

        // 行69「网络安全,Security,Exploit,Frameworks,Exp-Frameworks,...」
        // → name=Exp-Frameworks, parent=网络安全, target_path=Security\Exploit\Frameworks
        let row2: (String, String, String) = db
            .query_row(
                "SELECT name, parent, target_path FROM func_categories WHERE name = 'Exp-Frameworks'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("应存在 name=Exp-Frameworks 的记录");
        assert_eq!(row2.0, "Exp-Frameworks");
        assert_eq!(row2.1, "网络安全", "parent 应为中文");
        assert_eq!(row2.2, "Security\\Exploit\\Frameworks");

        // 验证 keywords 从描述提取：Boot 的 keywords 应含 EasyBCD
        let kw_str: String = db
            .query_row(
                "SELECT keywords FROM func_categories WHERE name = 'Boot'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(kw_str.contains("EasyBCD"), "keywords 应含 EasyBCD: {}", kw_str);
        assert!(kw_str.contains("系统引导"), "keywords 应含中文关键词: {}", kw_str);

        // 验证重复简写去重：Other 首个保留原名，重复的用 中文parent\简写
        let other_count: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM func_categories WHERE name = 'Other'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(other_count, 1, "首个 Other 保留原名");
        // 行66「在线工具,WebTool,Other,,Other」parent=在线工具，去重名 在线工具\Other
        let dedup_exists: i64 = db
            .query_row(
                "SELECT COUNT(*) FROM func_categories WHERE name = '在线工具\\Other'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(dedup_exists, 1, "重复 Other 应去重为 在线工具\\Other");
    }

    /// 验证 extract_keywords 提取规则
    #[test]
    fn test_extract_keywords() {
        let easy = "EasyBCD".to_string();
        // 英文专有名词应被提取
        let kw = extract_keywords("系统引导管理工具如EasyBCD、rEFInd、GRUB配置等");
        assert!(kw.contains(&easy), "应提取 EasyBCD: {:?}", kw);
        let reftxt = "rEFInd".to_string();
        assert!(kw.contains(&reftxt), "应提取 rEFInd: {:?}", kw);

        // 多个英文产品名
        let kw2 = extract_keywords("电路仿真工具如LTspice、Multisim仿真文件");
        assert!(kw2.iter().any(|k| k.contains("LTspice")), "应提取 LTspice: {:?}", kw2);

        // 纯中文描述应提取关键短语
        let kw3 = extract_keywords("系统清理工具包括垃圾清理、注册表清理");
        assert!(!kw3.is_empty(), "纯中文也应提取关键词: {:?}", kw3);

        // 空描述
        assert!(extract_keywords("").is_empty());

        // 关键词数量限制
        let long_desc = "工具如A、B、C、D、E、F、G、H、I、J、K等各种资源";
        let kw4 = extract_keywords(long_desc);
        assert!(kw4.len() <= 8, "关键词不超过 8 个: {}", kw4.len());
    }

    /// 验证 l1_to_cn 映射
    #[test]
    fn test_l1_to_cn() {
        assert_eq!(l1_to_cn("OS"), "操作系统");
        assert_eq!(l1_to_cn("Security"), "网络安全");
        assert_eq!(l1_to_cn("Wiki"), "知识库");
        assert_eq!(l1_to_cn("Unknown"), "Unknown"); // 未知则原样返回
    }
}
