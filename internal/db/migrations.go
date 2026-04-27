package db

import (
	"database/sql"
	"fmt"
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
	return nil
}
