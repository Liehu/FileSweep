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
	"github.com/google/uuid"
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
		(id, name, version, category, local_path, file_size, file_hash, extension, functional_category, status, ai_skip, scanned_at, mod_time, catalog_id)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		r.ID, r.Name, r.Version, r.Category, r.LocalPath,
		r.FileSize, r.FileHash, r.Extension, r.FunctionalCategory, r.Status, r.AISkip,
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
		(id, name, version, category, local_path, file_size, file_hash, extension, functional_category, status, ai_skip, scanned_at, mod_time, catalog_id)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`)
	if err != nil {
		return err
	}
	defer stmt.Close()

	for _, r := range records {
		_, err := stmt.Exec(
			r.ID, r.Name, r.Version, r.Category, r.LocalPath,
			r.FileSize, r.FileHash, r.Extension, r.FunctionalCategory, r.Status, r.AISkip,
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
	rows, err := c.db.Query("SELECT SUBSTR(name, 1, INSTR(name, '-') - 1) AS base, COUNT(DISTINCT version) AS ver_count FROM file_records WHERE version != '' AND INSTR(name, '-') > 1 GROUP BY base HAVING base != '' AND ver_count > 1")
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

// --- Tag CRUD ---

type TagEntry struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Color       string `json:"color"`
	Description string `json:"description"`
	Count       int    `json:"count"`
}

func (c *CatalogDB) GetTags() ([]TagEntry, error) {
	rows, err := c.db.Query(`
		SELECT t.id, t.name, t.color, t.description,
			(SELECT COUNT(*) FROM catalog_entries ce WHERE ce.tags LIKE '%"' || t.name || '"%') as cnt
		FROM tags t ORDER BY t.name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var tags []TagEntry
	for rows.Next() {
		var t TagEntry
		if err := rows.Scan(&t.ID, &t.Name, &t.Color, &t.Description, &t.Count); err != nil {
			continue
		}
		tags = append(tags, t)
	}
	return tags, nil
}

func (c *CatalogDB) GetAllTagNames() ([]string, error) {
	rows, err := c.db.Query(`SELECT name FROM tags ORDER BY name`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var names []string
	for rows.Next() {
		var name string
		if err := rows.Scan(&name); err != nil {
			continue
		}
		names = append(names, name)
	}
	return names, nil
}

func (c *CatalogDB) InsertTag(t TagEntry) (TagEntry, error) {
	if t.ID == "" {
		t.ID = "tag_" + uuid.New().String()[:8]
	}
	if t.Color == "" {
		t.Color = "#185FA5"
	}
	_, err := c.db.Exec(
		`INSERT OR IGNORE INTO tags (id, name, color, description) VALUES (?, ?, ?, ?)`,
		t.ID, t.Name, t.Color, t.Description,
	)
	return t, err
}

func (c *CatalogDB) UpdateTag(t TagEntry) error {
	_, err := c.db.Exec(
		`UPDATE tags SET name=?, color=?, description=? WHERE id=?`,
		t.Name, t.Color, t.Description, t.ID,
	)
	return err
}

func (c *CatalogDB) DeleteTag(id string) error {
	_, err := c.db.Exec(`DELETE FROM tags WHERE id = ?`, id)
	return err
}

// NormalizeTags filters AI tags against allowed set; returns ["others"] if none match.
func NormalizeTags(aiTags []string, allowedNames []string) []string {
	if len(allowedNames) == 0 {
		return aiTags
	}
	allowed := make(map[string]bool, len(allowedNames))
	for _, n := range allowedNames {
		allowed[n] = true
	}
	var result []string
	for _, t := range aiTags {
		if allowed[t] {
			result = append(result, t)
		}
	}
	if len(result) == 0 {
		return []string{"others"}
	}
	return result
}

// NormalizeFunctionalCategory enforces category is in allowed list, else "others".
func NormalizeFunctionalCategory(aiCategory string, allowedCategories []string) string {
	if len(allowedCategories) == 0 {
		return aiCategory
	}
	for _, c := range allowedCategories {
		if c == aiCategory {
			return aiCategory
		}
	}
	return "others"
}

// SeedDefaultTags populates tags table with common defaults if empty.
func (c *CatalogDB) SeedDefaultTags() error {
	var count int
	c.db.QueryRow("SELECT COUNT(*) FROM tags").Scan(&count)
	if count > 0 {
		return nil
	}
	names := []string{
		// Domain tags (from catalog-all.csv 一级分类)
		"os", "eda", "media", "sysenhance", "dev", "office", "game", "web", "webtool", "security", "wiki",
		// Function tags (from 二级分类 keywords)
		"installer", "portable", "config", "script", "editor", "player", "scanner", "vpn",
		"remote", "download", "compress", "recovery", "backup", "sync", "screenshot", "recorder",
		"design", "note", "ai-tool", "emulator", "password-manager", "file-manager",
		// Security L3 short codes (from catalog-all.csv Security 三级分类)
		"Exp-Frameworks", "Exp-Payload", "Exp-Browser", "Exp-Social", "Exp-Wireless",
		"Disc-Port", "Disc-Subd", "Disc-Path", "Disc-API", "Disc-Cloud",
		"Per-VPN", "Per-FW", "Per-Traffic", "Per-DNS", "Per-LB",
		"Creds-Pass", "Creds-AD", "Creds-Token", "Creds-SSO", "Creds-MFA",
		"Mid-Web", "Mid-App", "Mid-Cache", "Mid-BigData", "Mid-Queue", "Mid-Proxy",
		"Cloud-K8s", "Cloud-Container", "Cloud-Mesh",
		"Eva-Dynamic", "Eva-Static", "Eva-Fileless",
		"CA-PHP", "CA-Java", "CA-NET", "CA-Python", "CA-Go", "CA-CPP", "CA-Web",
		"Rev-Bin", "Rev-Android", "Rev-iOS", "Rev-Harmony", "Rev-Deobf", "Rev-Bytecode", "Rev-Firmware", "Rev-Kernel", "Rev-Wasm",
		"SC-Malicious", "SC-SCA", "SC-CICD", "SC-SBOM",
		"AI-LLM", "AI-Backdoor", "AI-Adversarial", "AI-Extraction", "AI-Privacy", "AI-SupplyChain", "AI-Hardening",
		"ICS-Protocol", "ICS-SCADA", "ICS-Firmware", "ICS-IDS", "ICS-Physical",
		"BT-EDR", "BT-Network", "BT-SIEM", "BT-Honeypot",
		"IoT-Wireless", "IoT-RTOS", "IoT-Smart", "IoT-LPWAN",
		"Word-Password", "Word-Vulnerability", "Word-Path", "Word-Parameter", "Word-Subdomain",
		"Priv-Discovery", "Priv-Anonymize", "Priv-Compliance",
		"Fore-Memory", "Fore-Disk", "Fore-Log", "Fore-Network",
		"Stg-Image", "Stg-Audio", "Stg-Video", "Stg-Document",
		"Pwn-Stack", "Pwn-Heap", "Pwn-Format", "Pwn-Integer",
		"Crypt-Classic", "Crypt-Symmetric", "Crypt-Asymmetric", "Crypt-Hash", "Crypt-Protocol",
		// Common security tags
		"exploit", "pentest", "forensics", "reverse", "ctf", "crypto",
		"pwn", "redteam", "blueteam", "vulnerability", "wordlist", "steganography",
		"code-audit", "evasion", "cloud-security", "iot-security", "mobile-security",
		// General attribute tags
		"open-source", "commercial", "utility", "server", "client", "cross-platform",
	}
	for i, name := range names {
		color := tagColor(i)
		t := TagEntry{Name: name, Color: color}
		if _, err := c.InsertTag(t); err != nil {
			return fmt.Errorf("seed tag %s: %w", name, err)
		}
	}
	return nil
}

func tagColor(i int) string {
	palette := []string{
		"#1D4ED8", "#7C3AED", "#0891B2", "#059669", "#185FA5", "#B45309", "#DC2626", "#6B7280",
		"#8B5CF6", "#A32D2D", "#3B6D11", "#2563EB", "#EA580C", "#D97706", "#854F0B", "#0E7490",
		"#65A30D", "#4338CA", "#059669", "#DC2626", "#0D9488", "#CA8A04", "#16A34A", "#9333EA",
		"#BE185D", "#0284C7", "#6D28D9", "#15803D", "#92400E", "#0369A1", "#374151", "#4B5563",
	}
	return palette[i%len(palette)]
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
		"SELECT id, name, version, category, local_path, file_size, file_hash, extension, functional_category, status, ai_skip, scanned_at, mod_time, catalog_id FROM file_records %s ORDER BY scanned_at DESC LIMIT ? OFFSET ?",
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
			&r.FileSize, &r.FileHash, &r.Extension, &r.FunctionalCategory, &r.Status, &r.AISkip, &scannedAt, &modTime, &r.CatalogID)
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

func (c *CatalogDB) UpdateFileFunctionalCategory(id, functionalCategory string) error {
	_, err := c.db.Exec("UPDATE file_records SET functional_category = ? WHERE id = ?", functionalCategory, id)
	return err
}

func (c *CatalogDB) InsertCatalogEntry(e core.CatalogEntry) error {
	tags, _ := json.Marshal(e.Tags)
	_, err := c.db.Exec(
		`INSERT OR REPLACE INTO catalog_entries
		(id, name, description, homepage_url, download_url, latest_version, license, functional_category, tags, ai_confidence, ai_provider, meta_updated_at, notes, needs_review)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
		e.ID, e.Name, e.Description, e.HomepageURL, e.DownloadURL,
		e.LatestVersion, e.License, e.FunctionalCategory, string(tags), e.AIConfidence,
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
		"SELECT id, name, description, homepage_url, download_url, latest_version, license, functional_category, tags, ai_confidence, ai_provider, meta_updated_at, notes, needs_review FROM catalog_entries %s ORDER BY name LIMIT ? OFFSET ?",
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
			&e.LatestVersion, &e.License, &e.FunctionalCategory, &tagsStr, &e.AIConfidence,
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
		`UPDATE catalog_entries SET description=?, homepage_url=?, download_url=?, latest_version=?, license=?, functional_category=?, tags=?, ai_confidence=?, ai_provider=?, meta_updated_at=?, notes=?, needs_review=? WHERE id=?`,
		e.Description, e.HomepageURL, e.DownloadURL,
		e.LatestVersion, e.License, e.FunctionalCategory, string(tags), e.AIConfidence,
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
