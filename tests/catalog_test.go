package tests

import (
	"path/filepath"
	"testing"
	"time"

	"filesweep/internal/core"
	"filesweep/internal/db"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func openTestDB(t *testing.T) *db.CatalogDB {
	t.Helper()
	path := filepath.Join(t.TempDir(), "test.db")
	database, err := db.Open(path)
	require.NoError(t, err)
	t.Cleanup(func() { database.Close() })
	return database
}

func TestDB_FileRecordCRUD(t *testing.T) {
	database := openTestDB(t)

	now := time.Now().UTC()
	records := []core.FileRecord{
		{
			ID: "rec_abc1_1000", Name: "python-3.11.0.exe", Version: "3.11.0",
			Category: "安装包", LocalPath: "python-3.11.0.exe", FileSize: 1000,
			FileHash: "hash1", Extension: ".exe", Status: "active", ScannedAt: now,
		},
		{
			ID: "rec_abc2_2000", Name: "deploy.sh", Version: "",
			Category: "脚本", LocalPath: "deploy.sh", FileSize: 2000,
			FileHash: "hash2", Extension: ".sh", Status: "active", ScannedAt: now,
		},
	}

	err := database.BatchInsertFileRecords(records)
	require.NoError(t, err)

	results, total, err := database.GetFileRecords("", "active", "", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 2, total)
	assert.Len(t, results, 2)

	results, total, err = database.GetFileRecords("安装包", "", "", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 1, total)
	assert.Equal(t, "python-3.11.0.exe", results[0].Name)

	results, total, err = database.GetFileRecords("", "", "deploy", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 1, total)

	err = database.UpdateFileStatus("rec_abc1_1000", "deleted")
	require.NoError(t, err)

	results, total, err = database.GetFileRecords("", "active", "", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 1, total)
}

func TestDB_CatalogEntryCRUD(t *testing.T) {
	database := openTestDB(t)

	entry := core.CatalogEntry{
		ID: "cat_nmap", Name: "nmap", Description: "网络扫描工具",
		HomepageURL: "https://nmap.org", DownloadURL: "https://nmap.org/download.html",
		LatestVersion: "7.95", License: "GPLv2", Tags: []string{"network", "security"},
		AIConfidence: 0.95, AIProvider: "openai", MetaUpdatedAt: time.Now().UTC(),
	}

	err := database.InsertCatalogEntry(entry)
	require.NoError(t, err)

	results, total, err := database.GetCatalogEntries("nmap", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 1, total)
	assert.Equal(t, "nmap", results[0].Name)
	assert.Equal(t, []string{"network", "security"}, results[0].Tags)

	entry.Description = "更新描述"
	entry.AIConfidence = 0.98
	err = database.UpdateCatalogEntry(entry)
	require.NoError(t, err)

	results, _, err = database.GetCatalogEntries("", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, "更新描述", results[0].Description)
}

func TestDB_OperationLogs(t *testing.T) {
	database := openTestDB(t)

	log := core.OperationLog{
		Timestamp: time.Now().UTC(), Operation: "DELETE",
		SourcePath: "/test/file.exe", Reason: "hash_match",
		FileHash: "abc", FileSize: 100, Status: "success",
		SessionID: "sess1", CanRevert: true,
	}

	err := database.InsertOperationLog(log)
	require.NoError(t, err)

	logs, total, err := database.GetOperationLogs("sess1", "", "", "", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 1, total)
	assert.Equal(t, core.Operation("DELETE"), logs[0].Operation)
	assert.Equal(t, "sess1", logs[0].SessionID)

	_, total, err = database.GetOperationLogs("other", "", "", "", 1, 10)
	require.NoError(t, err)
	assert.Equal(t, 0, total)
}
