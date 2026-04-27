package core

import (
	"encoding/csv"
	"encoding/json"
	"fmt"
	"os"
	"strconv"
)

func ExportCSV(logs []OperationLog, outputPath string) error {
	f, err := os.Create(outputPath)
	if err != nil {
		return fmt.Errorf("创建 CSV 文件失败: %w", err)
	}
	defer f.Close()

	w := csv.NewWriter(f)
	defer w.Flush()

	header := []string{"timestamp", "operation", "source_path", "dest_path", "reason", "file_hash", "file_size", "status", "session_id", "can_revert"}
	if err := w.Write(header); err != nil {
		return err
	}

	for _, l := range logs {
		record := []string{
			l.Timestamp.Format("2006-01-02T15:04:05Z"),
			string(l.Operation),
			l.SourcePath,
			l.DestPath,
			l.Reason,
			l.FileHash,
			strconv.FormatInt(l.FileSize, 10),
			l.Status,
			l.SessionID,
			strconv.FormatBool(l.CanRevert),
		}
		if err := w.Write(record); err != nil {
			return err
		}
	}
	return nil
}

func ExportCatalogCSV(records []FileRecord, entriesMap map[string]CatalogEntry, outputPath string) error {
	f, err := os.Create(outputPath)
	if err != nil {
		return fmt.Errorf("创建 CSV 文件失败: %w", err)
	}
	defer f.Close()

	w := csv.NewWriter(f)
	defer w.Flush()

	header := []string{
		"id", "name", "version", "category", "local_path", "file_size", "file_hash", "ai_skip",
		"description", "homepage_url", "download_url", "latest_version", "license", "tags",
		"ai_confidence", "meta_updated_at", "notes",
	}
	if err := w.Write(header); err != nil {
		return err
	}

	for _, r := range records {
		entry, hasEntry := entriesMap[r.CatalogID]

		var description, homepageURL, downloadURL, latestVersion, license, tagsStr, metaUpdatedAt, notes string
		var aiConfidence string

		if hasEntry {
			description = entry.Description
			homepageURL = entry.HomepageURL
			downloadURL = entry.DownloadURL
			latestVersion = entry.LatestVersion
			license = entry.License
			tagsBytes, _ := json.Marshal(entry.Tags)
			tagsStr = string(tagsBytes)
			aiConfidence = fmt.Sprintf("%.2f", entry.AIConfidence)
			metaUpdatedAt = entry.MetaUpdatedAt.Format("2006-01-02T15:04:05Z")
			notes = entry.Notes
		}

		row := []string{
			r.ID, r.Name, r.Version, r.Category, r.LocalPath,
			strconv.FormatInt(r.FileSize, 10), r.FileHash, strconv.FormatBool(r.AISkip),
			description, homepageURL, downloadURL, latestVersion, license, tagsStr,
			aiConfidence, metaUpdatedAt, notes,
		}
		if err := w.Write(row); err != nil {
			return err
		}
	}
	return nil
}
