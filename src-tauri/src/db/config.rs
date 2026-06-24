//! 配置 DB 化的 CRUD 方法（4 张表）
//!
//! - `software_roots`：软件安装根路径
//! - `category_rules`：分类规则（替代 rules.yaml）
//! - `func_categories`：功能分类（替代 categories.yaml）
//! - `exclude_rules`：排除规则（统一表）
//!
//! 所有方法挂在 `CatalogDB` 上，遵循现有 catalog.rs 的风格：
//! `&self.conn.lock().unwrap()` 拿连接，返回 `SqlResult<T>`。

use rusqlite::{params, Connection, Result as SqlResult};

use crate::db::catalog::CatalogDB;

// ────────────────── Models ──────────────────

/// 软件安装根路径（software_roots 表）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SoftwareRoot {
    pub id: i64,
    pub path: String,
    pub enabled: bool,
    pub display_name: String,
}

/// 分类规则（category_rules 表，替代 rules.yaml）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CategoryRuleRow {
    pub id: i64,
    pub name: String,
    pub target_path: String,
    pub extensions: Vec<String>,
    pub app_dir_only: bool,
    pub priority: i32,
    pub enabled: bool,
}

/// 功能分类（func_categories 表，替代 categories.yaml）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FuncCategoryRow {
    pub id: i64,
    pub name: String,
    pub keywords: Vec<String>,
    pub parent: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub target_path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// 排除规则（exclude_rules 表）
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExcludeRule {
    pub id: i64,
    pub rule_type: String, // "dir" / "ext" / "name"
    pub pattern: String,
    pub enabled: bool,
}

// ────────────────── CatalogDB impl ──────────────────

impl CatalogDB {
    // ═══════════════════ software_roots ═══════════════════

    pub fn list_software_roots(&self) -> SqlResult<Vec<SoftwareRoot>> {
        let conn = self.conn.lock().unwrap();
        list_software_roots_inner(&conn)
    }

    pub fn add_software_root(&self, path: &str, display_name: &str) -> SqlResult<SoftwareRoot> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO software_roots (path, display_name) VALUES (?1, ?2)",
            params![path, display_name],
        )?;
        let id = conn.last_insert_rowid();
        Ok(SoftwareRoot {
            id,
            path: path.to_string(),
            enabled: true,
            display_name: display_name.to_string(),
        })
    }

    /// 更新软件根路径（传 None 的字段保持原值）
    pub fn update_software_root(
        &self,
        id: i64,
        path: Option<&str>,
        display_name: Option<&str>,
        enabled: Option<bool>,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        // 动态拼 SET 子句，只更新传入的字段
        let mut sets: Vec<&str> = Vec::new();
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(v) = path {
            sets.push("path = ?");
            p.push(Box::new(v.to_string()));
        }
        if let Some(v) = display_name {
            sets.push("display_name = ?");
            p.push(Box::new(v.to_string()));
        }
        if let Some(v) = enabled {
            sets.push("enabled = ?");
            p.push(Box::new(if v { 1 } else { 0 }));
        }
        if sets.is_empty() {
            return Ok(()); // 没字段要更新
        }
        let sql = format!("UPDATE software_roots SET {} WHERE id = ?", sets.join(", "));
        p.push(Box::new(id));
        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
        Ok(())
    }

    pub fn delete_software_root(&self, id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM software_roots WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 返回所有启用的软件根路径（扫描入口判断用）
    pub fn get_enabled_software_roots(&self) -> SqlResult<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT path FROM software_roots WHERE enabled = 1 ORDER BY id")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    // ═══════════════════ category_rules ═══════════════════

    pub fn list_category_rules(&self) -> SqlResult<Vec<CategoryRuleRow>> {
        let conn = self.conn.lock().unwrap();
        list_category_rules_inner(&conn)
    }

    pub fn add_category_rule(&self, r: &CategoryRuleRow) -> SqlResult<CategoryRuleRow> {
        let conn = self.conn.lock().unwrap();
        let exts = serde_json::to_string(&r.extensions).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO category_rules (name, target_path, extensions,
             app_dir_only, priority, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                r.name,
                r.target_path,
                exts,
                if r.app_dir_only { 1 } else { 0 },
                r.priority,
                if r.enabled { 1 } else { 0 },
            ],
        )?;
        let id = conn.last_insert_rowid();
        let mut out = r.clone();
        out.id = id;
        Ok(out)
    }

    pub fn update_category_rule(&self, r: &CategoryRuleRow) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let exts = serde_json::to_string(&r.extensions).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE category_rules SET
             name = ?1, target_path = ?2, extensions = ?3,
             app_dir_only = ?4, priority = ?5, enabled = ?6
             WHERE id = ?7",
            params![
                r.name,
                r.target_path,
                exts,
                if r.app_dir_only { 1 } else { 0 },
                r.priority,
                if r.enabled { 1 } else { 0 },
                r.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_category_rule(&self, id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM category_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 取启用的规则（按 priority DESC），供 Classifier::from_db 用
    pub fn get_enabled_category_rules(&self) -> SqlResult<Vec<CategoryRuleRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, target_path, extensions,
                    app_dir_only, priority, enabled
             FROM category_rules WHERE enabled = 1
             ORDER BY priority DESC, id ASC",
        )?;
        map_category_rules(&mut stmt, &[])
    }

    // ═══════════════════ func_categories ═══════════════════

    pub fn list_func_categories(&self) -> SqlResult<Vec<FuncCategoryRow>> {
        let conn = self.conn.lock().unwrap();
        list_func_categories_inner(&conn)
    }

    pub fn add_func_category(&self, c: &FuncCategoryRow) -> SqlResult<FuncCategoryRow> {
        let conn = self.conn.lock().unwrap();
        let kws = serde_json::to_string(&c.keywords).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "INSERT INTO func_categories (name, keywords, parent, description, target_path, enabled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                c.name,
                kws,
                c.parent,
                c.description,
                c.target_path,
                if c.enabled { 1 } else { 0 },
            ],
        )?;
        let id = conn.last_insert_rowid();
        let mut out = c.clone();
        out.id = id;
        Ok(out)
    }

    pub fn update_func_category(&self, c: &FuncCategoryRow) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let kws = serde_json::to_string(&c.keywords).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE func_categories SET
             name = ?1, keywords = ?2, parent = ?3, description = ?4, target_path = ?5, enabled = ?6
             WHERE id = ?7",
            params![
                c.name,
                kws,
                c.parent,
                c.description,
                c.target_path,
                if c.enabled { 1 } else { 0 },
                c.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_func_category(&self, id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM func_categories WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 取启用的功能分类，供 Classifier / enricher 用
    pub fn get_enabled_func_categories(&self) -> SqlResult<Vec<FuncCategoryRow>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, keywords, parent, description, target_path, enabled
             FROM func_categories WHERE enabled = 1
             ORDER BY id",
        )?;
        map_func_categories(&mut stmt, &[])
    }

    // ═══════════════════ exclude_rules ═══════════════════

    pub fn list_exclude_rules(&self) -> SqlResult<Vec<ExcludeRule>> {
        let conn = self.conn.lock().unwrap();
        list_exclude_rules_inner(&conn)
    }

    pub fn add_exclude_rule(&self, rule_type: &str, pattern: &str) -> SqlResult<ExcludeRule> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO exclude_rules (rule_type, pattern) VALUES (?1, ?2)",
            params![rule_type, pattern],
        )?;
        let id = conn.last_insert_rowid();
        Ok(ExcludeRule {
            id,
            rule_type: rule_type.to_string(),
            pattern: pattern.to_string(),
            enabled: true,
        })
    }

    pub fn update_exclude_rule(
        &self,
        id: i64,
        rule_type: Option<&str>,
        pattern: Option<&str>,
        enabled: Option<bool>,
    ) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let mut sets: Vec<&str> = Vec::new();
        let mut p: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
        if let Some(v) = rule_type {
            sets.push("rule_type = ?");
            p.push(Box::new(v.to_string()));
        }
        if let Some(v) = pattern {
            sets.push("pattern = ?");
            p.push(Box::new(v.to_string()));
        }
        if let Some(v) = enabled {
            sets.push("enabled = ?");
            p.push(Box::new(if v { 1 } else { 0 }));
        }
        if sets.is_empty() {
            return Ok(());
        }
        let sql = format!("UPDATE exclude_rules SET {} WHERE id = ?", sets.join(", "));
        p.push(Box::new(id));
        let refs: Vec<&dyn rusqlite::ToSql> = p.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
        Ok(())
    }

    pub fn delete_exclude_rule(&self, id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM exclude_rules WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// 取启用的排除规则，按类型分组返回（扫描时用）
    pub fn get_enabled_exclude_rules(&self) -> SqlResult<ExcludeRules> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT rule_type, pattern FROM exclude_rules WHERE enabled = 1",
        )?;
        let mut out = ExcludeRules::default();
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
            ))
        })?;
        for r in rows {
            let (rt, pat) = r?;
            match rt.as_str() {
                "dir" => out.dirs.push(pat),
                "ext" => out.exts.push(pat),
                "name" => out.names.push(pat),
                _ => {}
            }
        }
        Ok(out)
    }
}

/// 排除规则分组（扫描入口直接消费）
#[derive(Debug, Clone, Default)]
pub struct ExcludeRules {
    pub dirs: Vec<String>,
    pub exts: Vec<String>,
    pub names: Vec<String>,
}

// ────────────────── 内部辅助函数（用裸 Connection，便于后续独立连接复用）──────────────────

fn list_software_roots_inner(conn: &Connection) -> SqlResult<Vec<SoftwareRoot>> {
    let mut stmt = conn.prepare(
        "SELECT id, path, enabled, display_name FROM software_roots ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(SoftwareRoot {
            id: row.get(0)?,
            path: row.get(1)?,
            enabled: row.get::<_, i32>(2)? != 0,
            display_name: row.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn list_category_rules_inner(conn: &Connection) -> SqlResult<Vec<CategoryRuleRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, target_path, extensions,
                app_dir_only, priority, enabled
         FROM category_rules ORDER BY priority DESC, id ASC",
    )?;
    map_category_rules(&mut stmt, &[])
}

fn list_func_categories_inner(conn: &Connection) -> SqlResult<Vec<FuncCategoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, keywords, parent, description, target_path, enabled
         FROM func_categories ORDER BY id",
    )?;
    map_func_categories(&mut stmt, &[])
}

fn list_exclude_rules_inner(conn: &Connection) -> SqlResult<Vec<ExcludeRule>> {
    let mut stmt = conn.prepare(
        "SELECT id, rule_type, pattern, enabled FROM exclude_rules ORDER BY id",
    )?;
    let rows = stmt.query_map([], |row| {
        Ok(ExcludeRule {
            id: row.get(0)?,
            rule_type: row.get(1)?,
            pattern: row.get(2)?,
            enabled: row.get::<_, i32>(3)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn map_category_rules(
    stmt: &mut rusqlite::Statement,
    params: &[&dyn rusqlite::ToSql],
) -> SqlResult<Vec<CategoryRuleRow>> {
    let rows = stmt.query_map(params, |row| {
        let ext_str: String = row.get(3)?;
        Ok(CategoryRuleRow {
            id: row.get(0)?,
            name: row.get(1)?,
            target_path: row.get(2)?,
            extensions: serde_json::from_str(&ext_str).unwrap_or_default(),
            app_dir_only: row.get::<_, i32>(4)? != 0,
            priority: row.get(5)?,
            enabled: row.get::<_, i32>(6)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

fn map_func_categories(
    stmt: &mut rusqlite::Statement,
    params: &[&dyn rusqlite::ToSql],
) -> SqlResult<Vec<FuncCategoryRow>> {
    let rows = stmt.query_map(params, |row| {
        let kw_str: String = row.get(2)?;
        Ok(FuncCategoryRow {
            id: row.get(0)?,
            name: row.get(1)?,
            keywords: serde_json::from_str(&kw_str).unwrap_or_default(),
            parent: row.get(3)?,
            description: row.get(4)?,
            target_path: row.get(5)?,
            enabled: row.get::<_, i32>(6)? != 0,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}
