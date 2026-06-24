use rusqlite::{params, Connection, OptionalExtension, Result as SqlResult};
use std::sync::Mutex;

use crate::core::models::*;

// Re-export for callers that import from crate::db::catalog
pub use crate::core::models::{FileStats, TagEntry};

pub struct CatalogDB {
    pub(crate) db_path: String,
    pub(crate) conn: Mutex<Connection>,
}

impl Clone for CatalogDB {
    fn clone(&self) -> Self {
        Self {
            db_path: self.db_path.clone(),
            conn: Mutex::new(
                Connection::open(&self.db_path)
                    .expect("failed to open db for clone"),
            ),
        }
    }
}

impl CatalogDB {
    pub fn open(db_path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        crate::db::migrations::migrate(&conn)?;
        Ok(Self {
            db_path: db_path.to_string(),
            conn: Mutex::new(conn),
        })
    }

    pub fn reset(&self) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let tables = [
            "file_records",
            "catalog_entries",
            "operation_logs",
            "categories",
            "tags",
        ];
        for table in &tables {
            conn.execute_batch(&format!("DELETE FROM {};", table))?;
        }
        Ok(())
    }

    // ────────────────── File Records ──────────────────

    pub fn insert_file_record(&self, r: &FileRecord) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO file_records
             (id, name, version, category, local_path, file_size, file_hash,
              extension, functional_category, status, ai_skip, scanned_at,
              mod_time, catalog_id, is_app_dir, app_dir_path, app_dir_reason)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                r.id,
                r.name,
                r.version,
                r.category,
                r.local_path,
                r.file_size,
                r.file_hash,
                r.extension,
                r.functional_category,
                r.status,
                r.ai_skip,
                r.scanned_at.to_rfc3339(),
                r.mod_time.to_rfc3339(),
                r.catalog_id,
                r.is_app_dir,
                r.app_dir_path,
                r.app_dir_reason,
            ],
        )?;
        Ok(())
    }

    pub fn batch_insert_file_records(&self, records: &[FileRecord]) -> SqlResult<()> {
        // 用独立连接（不与查询竞争 Mutex），WAL 模式支持多连接并发
        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous = OFF;")?;
        // 扫描是全量替换：DROP INDEX + DELETE + INSERT + CREATE INDEX
        conn.execute_batch(
            "DROP INDEX IF EXISTS idx_file_records_hash;
             DROP INDEX IF EXISTS idx_file_records_category;
             DROP INDEX IF EXISTS idx_file_records_status;
             DROP INDEX IF EXISTS idx_file_records_scanned;",
        )?;
        conn.execute_batch("DELETE FROM file_records;")?;
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO file_records
                 (id, name, version, category, local_path, file_size, file_hash,
                  extension, functional_category, status, ai_skip, scanned_at,
                  mod_time, catalog_id, is_app_dir, app_dir_path, app_dir_reason,
                  action, move_target, app_executables)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
            )?;
            for r in records {
                let execs_json = if r.app_executables.is_empty() {
                    "[]".to_string()
                } else {
                    serde_json::to_string(&r.app_executables).unwrap_or_else(|_| "[]".to_string())
                };
                stmt.execute(params![
                    r.id,
                    r.name,
                    r.version,
                    r.category,
                    r.local_path,
                    r.file_size,
                    r.file_hash,
                    r.extension,
                    r.functional_category,
                    r.status,
                    r.ai_skip,
                    r.scanned_at.to_rfc3339(),
                    r.mod_time.to_rfc3339(),
                    r.catalog_id,
                    r.is_app_dir,
                    r.app_dir_path,
                    r.app_dir_reason,
                    r.action,
                    r.move_target,
                    execs_json,
                ])?;
            }
        }
        tx.commit()?;
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_file_records_hash ON file_records(file_hash);
             CREATE INDEX IF NOT EXISTS idx_file_records_category ON file_records(category);
             CREATE INDEX IF NOT EXISTS idx_file_records_status ON file_records(status);
             CREATE INDEX IF NOT EXISTS idx_file_records_scanned ON file_records(scanned_at);",
        )?;
        conn.execute_batch("PRAGMA synchronous = NORMAL;")?;
        Ok(())
    }

    pub fn get_file_records(
        &self,
        category: &str,
        status: &str,
        search: &str,
        page: i32,
        page_size: i32,
    ) -> SqlResult<(Vec<FileRecord>, i32)> {
        // 用独立连接避免 Mutex 竞争
        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        let mut where_clauses: Vec<String> = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if !category.is_empty() {
            where_clauses.push("category = ?".to_string());
            param_values.push(Box::new(category.to_string()));
        }
        if !status.is_empty() {
            where_clauses.push("status = ?".to_string());
            param_values.push(Box::new(status.to_string()));
        }
        if !search.is_empty() {
            where_clauses.push("name LIKE ?".to_string());
            param_values.push(Box::new(format!("%{}%", search)));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let count_sql = format!("SELECT COUNT(*) as cnt FROM file_records {}", where_sql);
        let count: i32 = conn
            .query_row(&count_sql, param_refs.as_slice(), |row| row.get(0))?;

        // Paginated query
        let offset = (page - 1) * page_size;
        let data_sql = format!(
            "SELECT id, name, version, category, local_path, file_size, file_hash,
                    extension, functional_category, status, ai_skip, scanned_at,
                    mod_time, catalog_id, is_app_dir, app_dir_path, app_dir_reason,
                    action, move_target, app_executables
             FROM file_records {}
             ORDER BY scanned_at DESC
             LIMIT ? OFFSET ?",
            where_sql
        );

        let mut params_with_page: Vec<Box<dyn rusqlite::types::ToSql>> = param_values;
        params_with_page.push(Box::new(page_size));
        params_with_page.push(Box::new(offset));
        let page_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_with_page.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&data_sql)?;
        let rows = stmt.query_map(page_refs.as_slice(), |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                version: row.get(2)?,
                category: row.get(3)?,
                local_path: row.get(4)?,
                file_size: row.get(5)?,
                file_hash: row.get(6)?,
                extension: row.get(7)?,
                functional_category: row.get(8)?,
                status: row.get(9)?,
                ai_skip: row.get::<_, i32>(10)? != 0,
                scanned_at: parse_datetime(row.get::<_, String>(11)?),
                mod_time: parse_datetime(row.get::<_, String>(12)?),
                catalog_id: row.get::<_, String>(13).unwrap_or_default(),
                is_app_dir: row.get::<_, i32>(14).unwrap_or(0) != 0,
                app_dir_path: row.get::<_, String>(15).unwrap_or_default(),
                app_dir_reason: row.get::<_, String>(16).unwrap_or_default(),
                action: row.get::<_, String>(17).unwrap_or_default(),
                move_target: row.get::<_, String>(18).unwrap_or_default(),
                app_executables: row
                    .get::<_, String>(19)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
            })
        })?;

        let mut records = Vec::new();
        for row in rows {
            records.push(row?);
        }

        Ok((records, count))
    }

    pub fn update_file_status(&self, id: &str, status: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_records SET status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    /// 设置单文件的清理动作（delete/keep/move）及移动目标
    pub fn set_file_action(
        &self,
        id: &str,
        action: &str,
        move_target: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE file_records SET action = ?1, move_target = ?2 WHERE id = ?3",
            params![action, move_target.unwrap_or(""), id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// 批量设置清理动作
    pub fn batch_set_action(
        &self,
        ids: &[String],
        action: &str,
        move_target: Option<&str>,
    ) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut count = 0;
        for id in ids {
            conn.execute(
                "UPDATE file_records SET action = ?1, move_target = ?2 WHERE id = ?3",
                params![action, move_target.unwrap_or(""), id],
            )
            .map_err(|e| e.to_string())?;
            count += 1;
        }
        Ok(count)
    }

    pub fn update_file_functional_category(&self, id: &str, fc: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_records SET functional_category = ?1 WHERE id = ?2",
            params![fc, id],
        )?;
        Ok(())
    }

    pub fn delete_file_record(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM file_records WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ────────────────── File Stats ──────────────────

    pub fn get_file_stats(&self) -> SqlResult<FileStats> {
        let conn = rusqlite::Connection::open(&self.db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;

        let total: i64 =
            conn.query_row("SELECT COUNT(*) FROM file_records", [], |r| r.get(0))?;
        let total_size: i64 = conn
            .query_row("SELECT COALESCE(SUM(file_size), 0) FROM file_records", [], |r| {
                r.get(0)
            })?;

        // Duplicates: files whose hash appears more than once
        let duplicates: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_records WHERE file_hash IN (
                SELECT file_hash FROM file_records GROUP BY file_hash HAVING COUNT(*) > 1
            )",
            [],
            |r| r.get(0),
        )?;

        // Multi-version: files whose hash appears more than once
        let _multiversion: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_records WHERE id IN (
                SELECT fr.id FROM file_records fr
                INNER JOIN (
                    SELECT
                        CASE
                            WHEN INSTR(name, ' v') > 0 THEN SUBSTR(name, 1, INSTR(name, ' v'))
                            WHEN INSTR(name, ' V') > 0 THEN SUBSTR(name, 1, INSTR(name, ' V'))
                            WHEN INSTR(name, '-v') > 0 THEN SUBSTR(name, 1, INSTR(name, '-v'))
                            WHEN INSTR(name, '_v') > 0 THEN SUBSTR(name, 1, INSTR(name, '_v'))
                            WHEN INSTR(name, '-') > 0 AND CAST(
                                REPLACE(SUBSTR(name, INSTR(name, '-') + 1), '.',
                                    REPLACE(SUBSTR(name, INSTR(name, '-') + 1), '.', '')
                                ) AS INTEGER
                            ) IS NOT NULL
                            THEN SUBSTR(name, 1, INSTR(name, '-') - 1)
                            ELSE name
                        END AS base_name
                    FROM file_records
                ) bn ON 1=1
                WHERE fr.id != bn.ROWID
                GROUP BY fr.id
                LIMIT 1
            )",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        // Simpler multi-version count: files that have same name prefix but different versions
        let multiversion: i64 = conn.query_row(
            "SELECT COUNT(*) FROM (
                SELECT id FROM file_records
                WHERE (
                    SELECT COUNT(*)
                    FROM file_records fr2
                    WHERE file_records.name LIKE fr2.name || '%'
                       OR fr2.name LIKE file_records.name || '%'
                ) > 1
            )",
            [],
            |r| r.get(0),
        ).unwrap_or(0);

        let uncategorized: i64 = conn.query_row(
            "SELECT COUNT(*) FROM file_records
             WHERE (category IS NULL OR category = '') AND (functional_category IS NULL OR functional_category = '')",
            [],
            |r| r.get(0),
        )?;

        Ok(FileStats {
            total,
            total_size,
            duplicates,
            multiversion,
            uncategorized,
        })
    }

    // ────────────────── Catalog Entries ──────────────────

    pub fn insert_catalog_entry(&self, e: &CatalogEntry) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let tags_json = serde_json::to_string(&e.tags).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT OR REPLACE INTO catalog_entries
             (id, name, description, homepage_url, download_url, latest_version,
              license, functional_category, tags, ai_confidence, ai_provider,
              meta_updated_at, notes, needs_review, ai_skip)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
            params![
                e.id,
                e.name,
                e.description,
                e.homepage_url,
                e.download_url,
                e.latest_version,
                e.license,
                e.functional_category,
                tags_json,
                e.ai_confidence,
                e.ai_provider,
                e.meta_updated_at.to_rfc3339(),
                e.notes,
                e.needs_review,
                e.ai_skip,
            ],
        )?;
        Ok(())
    }

    pub fn get_catalog_entries(
        &self,
        search: &str,
        page: i32,
        page_size: i32,
    ) -> SqlResult<(Vec<CatalogEntry>, i32)> {
        let conn = self.conn.lock().unwrap();

        let (where_sql, search_param) = if search.is_empty() {
            (String::new(), Box::new(String::new()) as Box<dyn rusqlite::types::ToSql>)
        } else {
            (
                "WHERE name LIKE ?1 OR description LIKE ?1 OR tags LIKE ?1".to_string(),
                Box::new(format!("%{}%", search)) as Box<dyn rusqlite::types::ToSql>,
            )
        };

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = if search.is_empty() {
            vec![]
        } else {
            vec![search_param.as_ref()]
        };

        let count_sql = format!(
            "SELECT COUNT(*) FROM catalog_entries {}",
            where_sql
        );
        let count: i32 = conn.query_row(&count_sql, param_refs.as_slice(), |r| r.get(0))?;

        let offset = (page - 1) * page_size;

        let mut query_params: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        if !search.is_empty() {
            query_params.push(Box::new(format!("%{}%", search)));
        }
        let limit_idx = query_params.len() + 1;
        let offset_idx = limit_idx + 1;
        query_params.push(Box::new(page_size));
        query_params.push(Box::new(offset));

        let data_sql = format!(
            "SELECT id, name, description, homepage_url, download_url, latest_version,
                    license, functional_category, tags, ai_confidence, ai_provider,
                    meta_updated_at, notes, needs_review, ai_skip
             FROM catalog_entries {}
             ORDER BY meta_updated_at DESC
             LIMIT ?{} OFFSET ?{}",
            where_sql, limit_idx, offset_idx
        );
        let q_refs: Vec<&dyn rusqlite::types::ToSql> =
            query_params.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&data_sql)?;
        let rows = stmt.query_map(q_refs.as_slice(), |row| {
            let tags_str: String = row.get(8)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(CatalogEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                homepage_url: row.get(3)?,
                download_url: row.get(4)?,
                latest_version: row.get(5)?,
                license: row.get(6)?,
                functional_category: row.get(7)?,
                tags,
                ai_confidence: row.get(9)?,
                ai_provider: row.get(10)?,
                meta_updated_at: parse_datetime(row.get::<_, String>(11)?),
                notes: row.get(12)?,
                needs_review: row.get::<_, i32>(13)? != 0,
                ai_skip: row.get::<_, i32>(14)? != 0,
            })
        })?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        Ok((entries, count))
    }

    pub fn get_catalog_entry_by_id(&self, id: &str) -> SqlResult<Option<CatalogEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, homepage_url, download_url, latest_version,
                    license, functional_category, tags, ai_confidence, ai_provider,
                    meta_updated_at, notes, needs_review, ai_skip
             FROM catalog_entries WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            let tags_str: String = row.get(8)?;
            let tags: Vec<String> = serde_json::from_str(&tags_str).unwrap_or_default();
            Ok(CatalogEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                homepage_url: row.get(3)?,
                download_url: row.get(4)?,
                latest_version: row.get(5)?,
                license: row.get(6)?,
                functional_category: row.get(7)?,
                tags,
                ai_confidence: row.get(9)?,
                ai_provider: row.get(10)?,
                meta_updated_at: parse_datetime(row.get::<_, String>(11)?),
                notes: row.get(12)?,
                needs_review: row.get::<_, i32>(13)? != 0,
                ai_skip: row.get::<_, i32>(14)? != 0,
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn update_catalog_entry(&self, e: &CatalogEntry) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let tags_json = serde_json::to_string(&e.tags).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "UPDATE catalog_entries SET
             name = ?1, description = ?2, homepage_url = ?3, download_url = ?4,
             latest_version = ?5, license = ?6, functional_category = ?7, tags = ?8,
             ai_confidence = ?9, ai_provider = ?10, meta_updated_at = ?11,
             notes = ?12, needs_review = ?13, ai_skip = ?14
             WHERE id = ?15",
            params![
                e.name,
                e.description,
                e.homepage_url,
                e.download_url,
                e.latest_version,
                e.license,
                e.functional_category,
                tags_json,
                e.ai_confidence,
                e.ai_provider,
                e.meta_updated_at.to_rfc3339(),
                e.notes,
                e.needs_review,
                e.ai_skip,
                e.id,
            ],
        )?;
        Ok(())
    }

    pub fn delete_catalog_entry(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM catalog_entries WHERE id = ?1", params![id])?;
        Ok(())
    }

    // ────────────────── Operation Logs ──────────────────

    pub fn insert_operation_log(&self, l: &OperationLog) -> SqlResult<i64> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO operation_logs
             (timestamp, operation, source_path, dest_path, reason,
              file_hash, file_size, status, session_id, can_revert)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                l.timestamp.to_rfc3339(),
                l.operation,
                l.source_path,
                l.dest_path,
                l.reason,
                l.file_hash,
                l.file_size,
                l.status,
                l.session_id,
                l.can_revert,
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_operation_logs(
        &self,
        session_id: &str,
        action: &str,
        status: &str,
        q: &str,
        page: i32,
        page_size: i32,
    ) -> SqlResult<(Vec<OperationLog>, i32)> {
        let conn = self.conn.lock().unwrap();
        let mut where_clauses: Vec<String> = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if !session_id.is_empty() {
            where_clauses.push("session_id = ?".to_string());
            param_values.push(Box::new(session_id.to_string()));
        }
        if !action.is_empty() {
            where_clauses.push("operation = ?".to_string());
            param_values.push(Box::new(action.to_string()));
        }
        if !status.is_empty() {
            where_clauses.push("status = ?".to_string());
            param_values.push(Box::new(status.to_string()));
        }
        if !q.is_empty() {
            where_clauses.push("(source_path LIKE ? OR reason LIKE ?)".to_string());
            let q_pattern = format!("%{}%", q);
            param_values.push(Box::new(q_pattern.clone()));
            param_values.push(Box::new(q_pattern));
        }

        let where_sql = if where_clauses.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", where_clauses.join(" AND "))
        };

        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|b| b.as_ref()).collect();

        let count_sql = format!(
            "SELECT COUNT(*) FROM operation_logs {}",
            where_sql
        );
        let count: i32 = conn.query_row(&count_sql, param_refs.as_slice(), |r| r.get(0))?;

        let offset = (page - 1) * page_size;
        let data_sql = format!(
            "SELECT id, timestamp, operation, source_path, dest_path, reason,
                    file_hash, file_size, status, session_id, can_revert
             FROM operation_logs {}
             ORDER BY id DESC
             LIMIT ? OFFSET ?",
            where_sql
        );

        let mut params_with_page: Vec<Box<dyn rusqlite::types::ToSql>> = param_values;
        params_with_page.push(Box::new(page_size));
        params_with_page.push(Box::new(offset));
        let page_refs: Vec<&dyn rusqlite::types::ToSql> =
            params_with_page.iter().map(|b| b.as_ref()).collect();

        let mut stmt = conn.prepare(&data_sql)?;
        let rows = stmt.query_map(page_refs.as_slice(), |row| {
            Ok(OperationLog {
                id: row.get(0)?,
                timestamp: parse_datetime(row.get::<_, String>(1)?),
                operation: row.get(2)?,
                source_path: row.get(3)?,
                dest_path: row.get(4)?,
                reason: row.get(5)?,
                file_hash: row.get(6)?,
                file_size: row.get(7)?,
                status: row.get(8)?,
                session_id: row.get(9)?,
                can_revert: row.get::<_, i32>(10)? != 0,
            })
        })?;

        let mut logs = Vec::new();
        for row in rows {
            logs.push(row?);
        }

        Ok((logs, count))
    }

    pub fn get_operation_log_by_id(&self, id: i64) -> SqlResult<Option<OperationLog>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp, operation, source_path, dest_path, reason,
                    file_hash, file_size, status, session_id, can_revert
             FROM operation_logs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(OperationLog {
                id: row.get(0)?,
                timestamp: parse_datetime(row.get::<_, String>(1)?),
                operation: row.get(2)?,
                source_path: row.get(3)?,
                dest_path: row.get(4)?,
                reason: row.get(5)?,
                file_hash: row.get(6)?,
                file_size: row.get(7)?,
                status: row.get(8)?,
                session_id: row.get(9)?,
                can_revert: row.get::<_, i32>(10)? != 0,
            })
        })?;

        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn mark_log_reverted(&self, id: i64) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE operation_logs SET status = 'reverted', can_revert = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // ────────────────── Tags ──────────────────

    pub fn get_tags(&self) -> SqlResult<Vec<TagEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.color, t.description,
                    COALESCE((SELECT COUNT(*) FROM catalog_entries ce
                        WHERE ',' || ce.tags || ',' LIKE '%,' || t.name || ',%'), 0) AS cnt
             FROM tags t
             ORDER BY t.name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(TagEntry {
                id: row.get(0)?,
                name: row.get(1)?,
                color: row.get(2)?,
                description: row.get(3)?,
                count: row.get(4)?,
            })
        })?;

        let mut tags = Vec::new();
        for row in rows {
            tags.push(row?);
        }
        Ok(tags)
    }

    pub fn get_all_tag_names(&self) -> SqlResult<Vec<String>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT name FROM tags ORDER BY name")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        let mut names = Vec::new();
        for row in rows {
            names.push(row?);
        }
        Ok(names)
    }

    pub fn insert_tag(&self, tag: &TagEntry) -> SqlResult<TagEntry> {
        let conn = self.conn.lock().unwrap();
        let id = if tag.id.is_empty() {
            let uid = uuid::Uuid::new_v4();
            format!("tag_{}", &uid.to_string()[..8])
        } else {
            tag.id.clone()
        };
        conn.execute(
            "INSERT OR IGNORE INTO tags (id, name, color, description) VALUES (?1, ?2, ?3, ?4)",
            params![id, tag.name, tag.color, tag.description],
        )?;
        // If INSERT OR IGNORE skipped (duplicate name), fetch existing id
        let actual_id: String = conn.query_row(
            "SELECT id FROM tags WHERE name = ?1",
            params![tag.name],
            |row| row.get(0),
        ).unwrap_or(id);
        Ok(TagEntry {
            id: actual_id,
            name: tag.name.clone(),
            color: tag.color.clone(),
            description: tag.description.clone(),
            count: 0,
        })
    }

    pub fn update_tag(&self, tag: &TagEntry) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tags SET name = ?1, color = ?2, description = ?3 WHERE id = ?4",
            params![tag.name, tag.color, tag.description, tag.id],
        )?;
        Ok(())
    }

    pub fn delete_tag(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn normalize_tags(&self, ai_tags: &[String]) -> Vec<String> {
        let existing = self.get_all_tag_names().unwrap_or_default();
        normalize_tags(ai_tags, &existing)
    }

    pub fn seed_default_tags(&self) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let tags: &[(&str, &str, &str)] = &[
            // General
            ("cli", "#185FA5", "命令行界面工具"),
            ("gui", "#185FA5", "图形用户界面"),
            ("library", "#185FA5", "函数库/SDK"),
            ("framework", "#185FA5", "开发框架"),
            ("tool", "#185FA5", "通用工具"),
            ("utility", "#185FA5", "实用程序"),
            ("plugin", "#185FA5", "插件/扩展模块"),
            ("extension", "#185FA5", "浏览器/IDE 扩展"),
            ("driver", "#185FA5", "硬件驱动"),
            ("firmware", "#185FA5", "固件"),
            ("portable", "#185FA5", "便携版/免安装"),
            ("web", "#185FA5", "Web 应用/服务"),
            ("desktop", "#185FA5", "桌面应用"),
            ("mobile", "#185FA5", "移动端应用"),
            ("cross-platform", "#185FA5", "跨平台"),
            ("open-source", "#185FA5", "开源软件"),
            ("freeware", "#185FA5", "免费软件"),
            ("commercial", "#185FA5", "商业软件"),
            ("trial", "#185FA5", "试用版"),
            ("beta", "#185FA5", "测试版/预览版"),
            ("dev", "#185FA5", "开发版"),
            ("test", "#185FA5", "测试工具"),
            ("debug", "#185FA5", "调试工具"),
            // Network
            ("network", "#4CAF50", "网络工具"),
            ("proxy", "#4CAF50", "代理工具"),
            ("vpn", "#4CAF50", "VPN 工具"),
            ("dns", "#4CAF50", "DNS 工具"),
            ("firewall", "#4CAF50", "防火墙"),
            ("packet-capture", "#4CAF50", "抓包工具"),
            ("port-scanner", "#4CAF50", "端口扫描"),
            ("wifi", "#4CAF50", "WiFi 工具"),
            ("bluetooth", "#4CAF50", "蓝牙工具"),
            ("http", "#4CAF50", "HTTP 工具"),
            ("ftp", "#4CAF50", "FTP 工具"),
            ("ssh", "#4CAF50", "SSH 工具"),
            ("remote", "#4CAF50", "远程控制"),
            ("tunnel", "#4CAF50", "隧道工具"),
            ("download", "#4CAF50", "下载工具"),
            ("upload", "#4CAF50", "上传工具"),
            ("bandwidth", "#4CAF50", "带宽监控"),
            // Security
            ("security", "#F44336", "安全工具"),
            ("encryption", "#F44336", "加密工具"),
            ("authentication", "#F44336", "身份认证"),
            ("password", "#F44336", "密码管理"),
            ("certificate", "#F44336", "证书工具"),
            ("keygen", "#F44336", "密钥生成"),
            ("hash", "#F44336", "哈希计算"),
            ("forensics", "#F44336", "取证分析"),
            ("antivirus", "#F44336", "杀毒软件"),
            ("malware", "#F44336", "恶意软件分析"),
            ("exploit", "#F44336", "漏洞利用"),
            ("penetration", "#F44336", "渗透测试"),
            ("reverse-engineering", "#F44336", "逆向工程"),
            ("packet-analysis", "#F44336", "数据包分析"),
            ("vulnerability", "#F44336", "漏洞扫描"),
            ("ids", "#F44336", "入侵检测"),
            ("honeypot", "#F44336", "蜜罐"),
            ("sandbox", "#F44336", "沙箱环境"),
            // Development
            ("compiler", "#FF9800", "编译器"),
            ("interpreter", "#FF9800", "解释器"),
            ("debugger", "#FF9800", "调试器"),
            ("profiler", "#FF9800", "性能分析"),
            ("linter", "#FF9800", "代码检查"),
            ("formatter", "#FF9800", "代码格式化"),
            ("ide", "#FF9800", "集成开发环境"),
            ("editor", "#FF9800", "文本编辑器"),
            ("version-control", "#FF9800", "版本控制"),
            ("build-tool", "#FF9800", "构建工具"),
            ("package-manager", "#FF9800", "包管理器"),
            ("testing", "#FF9800", "测试框架"),
            ("ci-cd", "#FF9800", "CI/CD 工具"),
            ("deployment", "#FF9800", "部署工具"),
            ("container", "#FF9800", "容器化工具"),
            ("orchestration", "#FF9800", "编排工具"),
            ("api", "#FF9800", "API 工具"),
            ("database", "#FF9800", "数据库工具"),
            ("cache", "#FF9800", "缓存工具"),
            ("messaging", "#FF9800", "消息队列"),
            ("queue", "#FF9800", "队列系统"),
            // Media
            ("video", "#9C27B0", "视频工具"),
            ("audio", "#9C27B0", "音频工具"),
            ("image", "#9C27B0", "图像工具"),
            ("screenshot", "#9C27B0", "截图工具"),
            ("recording", "#9C27B0", "录屏/录制"),
            ("streaming", "#9C27B0", "流媒体"),
            ("converter", "#9C27B0", "格式转换"),
            ("media-editor", "#9C27B0", "媒体编辑器"),
            ("player", "#9C27B0", "媒体播放器"),
            ("viewer", "#9C27B0", "文件查看器"),
            // System
            ("backup", "#607D8B", "备份工具"),
            ("recovery", "#607D8B", "数据恢复"),
            ("disk", "#607D8B", "磁盘工具"),
            ("partition", "#607D8B", "分区工具"),
            ("monitor", "#607D8B", "系统监控"),
            ("benchmark", "#607D8B", "性能测试"),
            ("cleaner", "#607D8B", "系统清理"),
            ("optimizer", "#607D8B", "系统优化"),
            ("defragmenter", "#607D8B", "磁盘整理"),
            ("scheduler", "#607D8B", "任务调度"),
            ("automation", "#607D8B", "自动化工具"),
            ("service", "#607D8B", "系统服务"),
            ("registry", "#607D8B", "注册表工具"),
            // Documents
            ("pdf", "#795548", "PDF 工具"),
            ("reader", "#795548", "文档阅读器"),
            ("doc-converter", "#795548", "文档转换"),
            ("ocr", "#795548", "OCR 识别"),
            ("translator", "#795548", "翻译工具"),
            ("doc-editor", "#795548", "文档编辑器"),
            ("spreadsheet", "#795548", "电子表格"),
            ("presentation", "#795548", "演示文稿"),
        ];

        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT OR IGNORE INTO tags (id, name, color, description) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (name, color, desc) in tags {
                let id = format!("tag_{}", name);
                stmt.execute(params![id, *name, *color, *desc])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ────────────────── Categories (DB) ──────────────────

    pub fn get_categories(&self) -> SqlResult<Vec<Category>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, parent_id, target_path, extensions, name_keywords, sort_order
             FROM categories ORDER BY sort_order",
        )?;
        let rows = stmt.query_map([], |row| {
            let ext_str: String = row.get(4)?;
            let kw_str: String = row.get(5)?;
            Ok(Category {
                id: row.get(0)?,
                name: row.get(1)?,
                parent_id: row.get(2)?,
                target_path: row.get(3)?,
                extensions: serde_json::from_str(&ext_str).unwrap_or_default(),
                name_keywords: serde_json::from_str(&kw_str).unwrap_or_default(),
                sort_order: row.get(6)?,
            })
        })?;

        let mut categories = Vec::new();
        for row in rows {
            categories.push(row?);
        }
        Ok(categories)
    }

    pub fn insert_category(&self, c: &Category) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        let ext_json = serde_json::to_string(&c.extensions).unwrap_or_else(|_| "[]".to_string());
        let kw_json =
            serde_json::to_string(&c.name_keywords).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT OR REPLACE INTO categories
             (id, name, parent_id, target_path, extensions, name_keywords, sort_order)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![c.id, c.name, c.parent_id, c.target_path, ext_json, kw_json, c.sort_order],
        )?;
        Ok(())
    }

    pub fn update_category(&self, c: &Category) -> SqlResult<()> {
        self.insert_category(c)
    }

    pub fn delete_category(&self, id: &str) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM categories WHERE id = ?1", params![id])?;
        Ok(())
    }
}

// ────────────────── Free functions ──────────────────

/// Normalize AI-generated tags: keep those that match existing tag names (case-insensitive),
/// map unknown tags to "others".
pub fn normalize_tags(ai_tags: &[String], existing: &[String]) -> Vec<String> {
    let lower_existing: Vec<String> = existing.iter().map(|t| t.to_lowercase()).collect();

    let mut result = Vec::new();
    for tag in ai_tags {
        let tag_lower = tag.to_lowercase();
        let tag_trimmed = tag.trim().to_string();
        let tag_trimmed_lower = tag_trimmed.to_lowercase();

        if let Some(pos) = lower_existing.iter().position(|e| *e == tag_lower) {
            // Use the canonical casing from existing
            let canonical = &existing[pos];
            if !result.iter().any(|r: &String| r.to_lowercase() == tag_lower) {
                result.push(canonical.clone());
            }
        } else if tag_trimmed.is_empty() {
            continue;
        } else if let Some(pos) = lower_existing.iter().position(|e| {
            e.starts_with(&tag_trimmed_lower) || tag_trimmed_lower.starts_with(e)
        }) {
            let canonical = &existing[pos];
            if !result.iter().any(|r: &String| r.to_lowercase() == canonical.to_lowercase()) {
                result.push(canonical.clone());
            }
        } else if !result.iter().any(|r| r == "others") {
            result.push("others".to_string());
        }
    }
    result
}

/// Normalize a functional category name to match an existing category (case-insensitive).
pub fn normalize_functional_category(cat: &str, allowed: &[String]) -> String {
    if cat.is_empty() {
        return cat.to_string();
    }
    let cat_lower = cat.to_lowercase();
    for a in allowed {
        if a.to_lowercase() == cat_lower {
            return a.clone();
        }
    }
    // Prefix match fallback
    for a in allowed {
        if a.to_lowercase().starts_with(&cat_lower) || cat_lower.starts_with(&a.to_lowercase()) {
            return a.clone();
        }
    }
    cat.to_string()
}

// ────────────────── Helpers ──────────────────

fn parse_datetime(s: String) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|_| chrono::Utc::now())
}

// ────────────────── Category model (DB table) ──────────────────
// Defined here because it's only used by the DB layer, not by the core models.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub parent_id: String,
    pub target_path: String,
    pub extensions: Vec<String>,
    pub name_keywords: Vec<String>,
    pub sort_order: i32,
}
