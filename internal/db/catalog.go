package db

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"filesweep/internal/core"

	_ "modernc.org/sqlite"
)

type CatalogDB struct {
	db *sql.DB
}

func Open(dbPath string) (*CatalogDB, error) {
	dir := filepath.Dir(dbPath)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, fmt.Errorf("创建数据库目录失败: %w", err)
	}

	db, err := sql.Open("sqlite", dbPath)
	if err != nil {
		return nil, fmt.Errorf("打开数据库失败: %w", err)
	}

	db.SetMaxOpenConns(1)

	if err := Migrate(db); err != nil {
		db.Close()
		return nil, err
	}

	return &CatalogDB{db: db}, nil
}

func (c *CatalogDB) Close() error {
	return c.db.Close()
}

func (c *CatalogDB) InsertFileRecord(r core.FileRecord) error {
	_, err := c.db.Exec(
		`INSERT OR REPLACE INTO file_records
		(id, name, version, category, local_path, file_size, file_hash, extension, status, ai_skip, scanned_at, mod_time, catalog_id)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		r.ID, r.Name, r.Version, r.Category, r.LocalPath,
		r.FileSize, r.FileHash, r.Extension, r.Status, r.AISkip,
		r.ScannedAt.Format(time.RFC3339), r.ModTime.Format(time.RFC3339), r.CatalogID,
	)
	return err
}

func (c *CatalogDB) Reset() error {
	if _, err := c.db.Exec("DELETE FROM file_records"); err != nil {
		return err
	}
	_, err := c.db.Exec("DELETE FROM operation_logs")
	return err
}

func (c *CatalogDB) BatchInsertFileRecords(records []core.FileRecord) error {
	tx, err := c.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()

	stmt, err := tx.Prepare(
		`INSERT OR REPLACE INTO file_records
		(id, name, version, category, local_path, file_size, file_hash, extension, status, ai_skip, scanned_at, mod_time, catalog_id)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return err
	}
	defer stmt.Close()

	for _, r := range records {
		_, err := stmt.Exec(
			r.ID, r.Name, r.Version, r.Category, r.LocalPath,
			r.FileSize, r.FileHash, r.Extension, r.Status, r.AISkip,
			r.ScannedAt.Format(time.RFC3339), r.ModTime.Format(time.RFC3339), r.CatalogID,
		)
		if err != nil {
			return fmt.Errorf("插入文件记录失败 %s: %w", r.Name, err)
		}
	}
	return tx.Commit()
}

func (c *CatalogDB) DeleteFileRecord(id string) error {
	_, err := c.db.Exec(`DELETE FROM file_records WHERE id = ?`, id)
	return err
}

func (c *CatalogDB) UpdateFileRecordStatus(id string, status string) error {
	_, err := c.db.Exec(`UPDATE file_records SET status = ? WHERE id = ?`, status, id)
	return err
}

type FileStats struct {
	Total         int
	TotalSize     int64
	Duplicates    int
	Multiversion  int
	Uncategorized int
}

func (c *CatalogDB) GetFileStats() (*FileStats, error) {
	stats := &FileStats{}

	if err := c.db.QueryRow("SELECT COUNT(*), COALESCE(SUM(file_size), 0) FROM file_records").Scan(&stats.Total, &stats.TotalSize); err != nil {
		return nil, err
	}

	// Duplicates: count files sharing the same hash (excluding unique hashes)
	if err := c.db.QueryRow("SELECT COUNT(*) FROM file_records WHERE file_hash IN (SELECT file_hash FROM file_records WHERE file_hash != '' GROUP BY file_hash HAVING COUNT(*) > 1)").Scan(&stats.Duplicates); err != nil {
		// Non-critical, continue
		stats.Duplicates = 0
	}

	// Uncategorized
	if err := c.db.QueryRow("SELECT COUNT(*) FROM file_records WHERE category = '' OR category = 'unknown' OR category = '未分类'").Scan(&stats.Uncategorized); err != nil {
		stats.Uncategorized = 0
	}

	// Multi-version: files with same base name (name before first '-') but different versions
	rows, err := c.db.Query("SELECT SUBSTR(name, 1, INSTR(name, '-') - 1) AS base, COUNT(DISTINCT version) AS ver_count FROM file_records WHERE version != '' AND name LIKE '%-%' GROUP BY base HAVING ver_count > 1")
	if err == nil {
		defer rows.Close()
		for rows.Next() {
			var base string
			var verCount int
			if rows.Scan(&base, &verCount) == nil {
				stats.Multiversion += verCount - 1
			}
		}
	}

	return stats, nil
}

// --- Category CRUD ---

type Category struct {
	ID           string   `json:"id"`
	Name         string   `json:"name"`
	ParentID     string   `json:"parent_id"`
	TargetPath   string   `json:"target_path"`
	Extensions   []string `json:"extensions"`
	NameKeywords []string `json:"name_keywords"`
	SortOrder    int      `json:"sort_order"`
}

func (c *CatalogDB) GetCategories() ([]Category, error) {
	rows, err := c.db.Query(`SELECT id, name, parent_id, target_path, extensions, name_keywords, sort_order FROM categories ORDER BY sort_order, name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var cats []Category
	for rows.Next() {
		var cat Category
		var extJSON, kwJSON string
		if err := rows.Scan(&cat.ID, &cat.Name, &cat.ParentID, &cat.TargetPath, &extJSON, &kwJSON, &cat.SortOrder); err != nil {
			continue
		}
		json.Unmarshal([]byte(extJSON), &cat.Extensions)
		json.Unmarshal([]byte(kwJSON), &cat.NameKeywords)
		cats = append(cats, cat)
	}
	return cats, nil
}

func (c *CatalogDB) InsertCategory(cat Category) error {
	extJSON, _ := json.Marshal(cat.Extensions)
	kwJSON, _ := json.Marshal(cat.NameKeywords)
	_, err := c.db.Exec(
		`INSERT OR REPLACE INTO categories (id, name, parent_id, target_path, extensions, name_keywords, sort_order) VALUES (?, ?, ?, ?, ?, ?, ?)`,
		cat.ID, cat.Name, cat.ParentID, cat.TargetPath, string(extJSON), string(kwJSON), cat.SortOrder,
	)
	return err
}

func (c *CatalogDB) DeleteCategory(id string) error {
	_, err := c.db.Exec(`DELETE FROM categories WHERE id = ?`, id)
	return err
}

// --- Revert support ---

func (c *CatalogDB) GetOperationLogByID(id int64) (*core.OperationLog, error) {
	var log core.OperationLog
	err := c.db.QueryRow(`SELECT id, timestamp, operation, source_path, dest_path, reason, file_hash, file_size, status, session_id, can_revert FROM operation_logs WHERE id = ?`, id).
		Scan(&log.ID, &log.Timestamp, &log.Operation, &log.SourcePath, &log.DestPath, &log.Reason, &log.FileHash, &log.FileSize, &log.Status, &log.SessionID, &log.CanRevert)
	if err != nil {
		return nil, err
	}
	return &log, nil
}

func (c *CatalogDB) MarkLogReverted(id int64) error {
	_, err := c.db.Exec(`UPDATE operation_logs SET status = 'reverted', can_revert = false WHERE id = ?`, id)
	return err
}

func (c *CatalogDB) GetFileRecords(category, status, search string, page, pageSize int) ([]core.FileRecord, int, error) {
	var conditions []string
	var args []any

	if category != "" {
		conditions = append(conditions, "(category = ? OR category LIKE ?)")
		args = append(args, category, category+"\\%")
	}
	if status != "" {
		conditions = append(conditions, "status = ?")
		args = append(args, status)
	}
	if search != "" {
		conditions = append(conditions, "name LIKE ?")
		args = append(args, "%"+search+"%")
	}

	where := ""
	if len(conditions) > 0 {
		where = "WHERE " + strings.Join(conditions, " AND ")
	}

	var total int
	countSQL := fmt.Sprintf("SELECT COUNT(*) FROM file_records %s", where)
	if err := c.db.QueryRow(countSQL, args...).Scan(&total); err != nil {
		return nil, 0, err
	}

	if page < 1 {
		page = 1
	}
	offset := (page - 1) * pageSize

	querySQL := fmt.Sprintf(
		"SELECT id, name, version, category, local_path, file_size, file_hash, extension, status, ai_skip, scanned_at, mod_time, catalog_id FROM file_records %s ORDER BY scanned_at DESC LIMIT ? OFFSET ?",
		where,
	)
	args = append(args, pageSize, offset)

	rows, err := c.db.Query(querySQL, args...)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var records []core.FileRecord
	for rows.Next() {
		var r core.FileRecord
		var scannedAt, modTime string
		err := rows.Scan(&r.ID, &r.Name, &r.Version, &r.Category, &r.LocalPath,
			&r.FileSize, &r.FileHash, &r.Extension, &r.Status, &r.AISkip, &scannedAt, &modTime, &r.CatalogID)
		if err != nil {
			return nil, 0, err
		}
		r.ScannedAt, _ = time.Parse(time.RFC3339, scannedAt)
		r.ModTime, _ = time.Parse(time.RFC3339, modTime)
		records = append(records, r)
	}
	return records, total, nil
}

func (c *CatalogDB) UpdateFileStatus(id, status string) error {
	_, err := c.db.Exec("UPDATE file_records SET status = ? WHERE id = ?", status, id)
	return err
}

func (c *CatalogDB) InsertCatalogEntry(e core.CatalogEntry) error {
	tags, _ := json.Marshal(e.Tags)
	_, err := c.db.Exec(
		`INSERT OR REPLACE INTO catalog_entries
		(id, name, description, homepage_url, download_url, latest_version, license, tags, ai_confidence, ai_provider, meta_updated_at, notes, needs_review)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		e.ID, e.Name, e.Description, e.HomepageURL, e.DownloadURL,
		e.LatestVersion, e.License, string(tags), e.AIConfidence,
		e.AIProvider, e.MetaUpdatedAt.Format(time.RFC3339), e.Notes, e.NeedsReview,
	)
	return err
}

func (c *CatalogDB) GetCatalogEntries(search string, page, pageSize int) ([]core.CatalogEntry, int, error) {
	var conditions []string
	var args []any

	if search != "" {
		conditions = append(conditions, "(name LIKE ? OR description LIKE ? OR tags LIKE ?)")
		args = append(args, "%"+search+"%", "%"+search+"%", "%"+search+"%")
	}

	where := ""
	if len(conditions) > 0 {
		where = "WHERE " + strings.Join(conditions, " AND ")
	}

	var total int
	countSQL := fmt.Sprintf("SELECT COUNT(*) FROM catalog_entries %s", where)
	if err := c.db.QueryRow(countSQL, args...).Scan(&total); err != nil {
		return nil, 0, err
	}

	if page < 1 {
		page = 1
	}
	offset := (page - 1) * pageSize

	querySQL := fmt.Sprintf(
		"SELECT id, name, description, homepage_url, download_url, latest_version, license, tags, ai_confidence, ai_provider, meta_updated_at, notes, needs_review FROM catalog_entries %s ORDER BY name LIMIT ? OFFSET ?",
		where,
	)
	args = append(args, pageSize, offset)

	rows, err := c.db.Query(querySQL, args...)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var entries []core.CatalogEntry
	for rows.Next() {
		var e core.CatalogEntry
		var tagsStr string
		var metaUpdatedAt sql.NullString
		err := rows.Scan(&e.ID, &e.Name, &e.Description, &e.HomepageURL, &e.DownloadURL,
			&e.LatestVersion, &e.License, &tagsStr, &e.AIConfidence,
			&e.AIProvider, &metaUpdatedAt, &e.Notes, &e.NeedsReview)
		if err != nil {
			return nil, 0, err
		}
		json.Unmarshal([]byte(tagsStr), &e.Tags)
		if metaUpdatedAt.Valid {
			e.MetaUpdatedAt, _ = time.Parse(time.RFC3339, metaUpdatedAt.String)
		}
		entries = append(entries, e)
	}
	return entries, total, nil
}

func (c *CatalogDB) UpdateCatalogEntry(e core.CatalogEntry) error {
	tags, _ := json.Marshal(e.Tags)
	_, err := c.db.Exec(
		`UPDATE catalog_entries SET description=?, homepage_url=?, download_url=?, latest_version=?, license=?, tags=?, ai_confidence=?, ai_provider=?, meta_updated_at=?, notes=?, needs_review=? WHERE id=?`,
		e.Description, e.HomepageURL, e.DownloadURL,
		e.LatestVersion, e.License, string(tags), e.AIConfidence,
		e.AIProvider, time.Now().UTC().Format(time.RFC3339), e.Notes, e.NeedsReview, e.ID,
	)
	return err
}

func (c *CatalogDB) DeleteCatalogEntry(id string) error {
	_, err := c.db.Exec("DELETE FROM catalog_entries WHERE id = ?", id)
	return err
}

func (c *CatalogDB) InsertOperationLog(l core.OperationLog) error {
	_, err := c.db.Exec(
		`INSERT INTO operation_logs (timestamp, operation, source_path, dest_path, reason, file_hash, file_size, status, session_id, can_revert)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		l.Timestamp.Format(time.RFC3339), l.Operation, l.SourcePath, l.DestPath,
		l.Reason, l.FileHash, l.FileSize, l.Status, l.SessionID, l.CanRevert,
	)
	return err
}

func (c *CatalogDB) GetOperationLogs(sessionID, action, status, q string, page, pageSize int) ([]core.OperationLog, int, error) {
	var conditions []string
	var args []any

	if sessionID != "" {
		conditions = append(conditions, "session_id = ?")
		args = append(args, sessionID)
	}
	if action != "" {
		conditions = append(conditions, "operation = ?")
		args = append(args, strings.ToUpper(action))
	}
	if status != "" {
		conditions = append(conditions, "status = ?")
		args = append(args, status)
	}
	if q != "" {
		conditions = append(conditions, "(source_path LIKE ? OR dest_path LIKE ? OR reason LIKE ?)")
		args = append(args, "%"+q+"%", "%"+q+"%", "%"+q+"%")
	}

	where := ""
	if len(conditions) > 0 {
		where = "WHERE " + strings.Join(conditions, " AND ")
	}

	var total int
	countSQL := fmt.Sprintf("SELECT COUNT(*) FROM operation_logs %s", where)
	if err := c.db.QueryRow(countSQL, args...).Scan(&total); err != nil {
		return nil, 0, err
	}

	if page < 1 {
		page = 1
	}
	offset := (page - 1) * pageSize

	querySQL := fmt.Sprintf(
		"SELECT id, timestamp, operation, source_path, dest_path, reason, file_hash, file_size, status, session_id, can_revert FROM operation_logs %s ORDER BY timestamp DESC LIMIT ? OFFSET ?",
		where,
	)
	args = append(args, pageSize, offset)

	rows, err := c.db.Query(querySQL, args...)
	if err != nil {
		return nil, 0, err
	}
	defer rows.Close()

	var logs []core.OperationLog
	for rows.Next() {
		var l core.OperationLog
		var ts string
		err := rows.Scan(&l.ID, &ts, &l.Operation, &l.SourcePath, &l.DestPath,
			&l.Reason, &l.FileHash, &l.FileSize, &l.Status, &l.SessionID, &l.CanRevert)
		if err != nil {
			return nil, 0, err
		}
		l.Timestamp, _ = time.Parse(time.RFC3339, ts)
		logs = append(logs, l)
	}
	return logs, total, nil
}
