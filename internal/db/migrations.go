package db

import (
	"database/sql"
	"fmt"
	"log/slog"
	"strings"
)

func Migrate(db *sql.DB) error {
	migrations := []string{
		`CREATE TABLE IF NOT EXISTS file_records (
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
			ai_skip BOOLEAN DEFAULT FALSE,
			scanned_at DATETIME NOT NULL,
			mod_time DATETIME DEFAULT '',
			catalog_id TEXT,
			created_at DATETIME DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE TABLE IF NOT EXISTS catalog_entries (
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
			meta_updated_at DATETIME,
			notes TEXT DEFAULT '',
			needs_review BOOLEAN DEFAULT FALSE,
			created_at DATETIME DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE TABLE IF NOT EXISTS operation_logs (
			id INTEGER PRIMARY KEY AUTOINCREMENT,
			timestamp DATETIME NOT NULL,
			operation TEXT NOT NULL,
			source_path TEXT NOT NULL,
			dest_path TEXT DEFAULT '',
			reason TEXT DEFAULT '',
			file_hash TEXT DEFAULT '',
			file_size INTEGER DEFAULT 0,
			status TEXT DEFAULT 'success',
			session_id TEXT DEFAULT '',
			can_revert BOOLEAN DEFAULT FALSE
		)`,
		`CREATE INDEX IF NOT EXISTS idx_file_records_hash ON file_records(file_hash)`,
		`CREATE INDEX IF NOT EXISTS idx_file_records_category ON file_records(category)`,
		`CREATE INDEX IF NOT EXISTS idx_file_records_status ON file_records(status)`,
		`CREATE INDEX IF NOT EXISTS idx_operation_logs_session ON operation_logs(session_id)`,
		`CREATE INDEX IF NOT EXISTS idx_catalog_entries_name ON catalog_entries(name)`,
		`CREATE TABLE IF NOT EXISTS categories (
			id TEXT PRIMARY KEY,
			name TEXT NOT NULL,
			parent_id TEXT DEFAULT '',
			target_path TEXT DEFAULT '',
			extensions TEXT DEFAULT '[]',
			name_keywords TEXT DEFAULT '[]',
			sort_order INTEGER DEFAULT 0,
			created_at DATETIME DEFAULT CURRENT_TIMESTAMP
		)`,
		`CREATE INDEX IF NOT EXISTS idx_categories_parent ON categories(parent_id)`,
	}

	for _, m := range migrations {
		if _, err := db.Exec(m); err != nil {
			return fmt.Errorf("执行迁移失败: %w", err)
		}
	}

	// Schema patches: add missing columns to existing tables
	patches := []struct {
		table  string
		column string
		def    string
	}{
		{"file_records", "mod_time", "DATETIME DEFAULT ''"},
		{"file_records", "catalog_id", "TEXT"},
		{"file_records", "functional_category", "TEXT DEFAULT ''"},
		{"catalog_entries", "functional_category", "TEXT DEFAULT ''"},
	}

	for _, p := range patches {
		if !columnExists(db, p.table, p.column) {
			sql := fmt.Sprintf("ALTER TABLE %s ADD COLUMN %s %s", p.table, p.column, p.def)
			if _, err := db.Exec(sql); err != nil {
				slog.Warn("迁移补丁失败", "sql", sql, "error", err)
			} else {
				slog.Info("数据库补丁已应用", "table", p.table, "column", p.column)
			}
		}
	}

	return nil
}

func columnExists(db *sql.DB, table, column string) bool {
	rows, err := db.Query(fmt.Sprintf("PRAGMA table_info(%s)", table))
	if err != nil {
		return false
	}
	defer rows.Close()

	for rows.Next() {
		var cid int
		var name, ctype string
		var notnull int
		var dfltValue sql.NullString
		var pk int
		if err := rows.Scan(&cid, &name, &ctype, &notnull, &dfltValue, &pk); err != nil {
			continue
		}
		if strings.EqualFold(name, column) {
			return true
		}
	}
	return false
}
