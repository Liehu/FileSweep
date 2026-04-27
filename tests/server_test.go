package tests

import (
	"bytes"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"testing"
	"time"

	"filesweep/internal/config"
	"filesweep/internal/core"
	"filesweep/internal/db"
	"filesweep/internal/server"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func setupTestServer(t *testing.T) (*server.Server, *db.CatalogDB) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "test.db")
	database, err := db.Open(path)
	require.NoError(t, err)
	t.Cleanup(func() { database.Close() })

	cfg := &config.Config{
		RulesPath: "../config/rules.yaml",
	}

	srv := server.New(cfg, database, nil)
	return srv, database
}

func TestAPI_GetFiles_Empty(t *testing.T) {
	srv, _ := setupTestServer(t)

	w := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/api/files", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)

	var resp map[string]any
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Equal(t, float64(0), resp["total"])
}

func TestAPI_GetFiles_WithData(t *testing.T) {
	srv, database := setupTestServer(t)

	now := time.Now().UTC()
	database.BatchInsertFileRecords([]core.FileRecord{
		{ID: "rec_1", Name: "test.exe", Extension: ".exe", Category: "安装包", Status: "active", FileHash: "h1", FileSize: 100, ScannedAt: now},
		{ID: "rec_2", Name: "readme.md", Extension: ".md", Category: "文档", Status: "active", FileHash: "h2", FileSize: 50, ScannedAt: now},
	})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/api/files", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
	var resp map[string]any
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Equal(t, float64(2), resp["total"])
}

func TestAPI_GetFiles_FilterByCategory(t *testing.T) {
	srv, database := setupTestServer(t)

	now := time.Now().UTC()
	database.BatchInsertFileRecords([]core.FileRecord{
		{ID: "rec_1", Name: "test.exe", Category: "安装包", Status: "active", FileHash: "h1", FileSize: 100, ScannedAt: now, Extension: ".exe"},
		{ID: "rec_2", Name: "readme.md", Category: "文档", Status: "active", FileHash: "h2", FileSize: 50, ScannedAt: now, Extension: ".md"},
	})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/api/files?category=安装包", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
	var resp map[string]any
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Equal(t, float64(1), resp["total"])
}

func TestAPI_GetFiles_Search(t *testing.T) {
	srv, database := setupTestServer(t)

	now := time.Now().UTC()
	database.BatchInsertFileRecords([]core.FileRecord{
		{ID: "rec_1", Name: "nmap-7.94.exe", Status: "active", FileHash: "h1", FileSize: 100, ScannedAt: now, Extension: ".exe"},
		{ID: "rec_2", Name: "python-3.11.exe", Status: "active", FileHash: "h2", FileSize: 50, ScannedAt: now, Extension: ".exe"},
	})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/api/files?search=nmap", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
	var resp map[string]any
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Equal(t, float64(1), resp["total"])
}

func TestAPI_StartScan(t *testing.T) {
	srv, _ := setupTestServer(t)

	dir := t.TempDir()
	body, _ := json.Marshal(map[string]any{"dirs": []string{dir}, "recursive": false})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("POST", "/api/scan", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusAccepted, w.Code)
	var resp map[string]any
	json.Unmarshal(w.Body.Bytes(), &resp)
	assert.Equal(t, "scanning", resp["status"])
}

func TestAPI_StartScan_MissingDir(t *testing.T) {
	srv, _ := setupTestServer(t)

	body, _ := json.Marshal(map[string]any{})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("POST", "/api/scan", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusBadRequest, w.Code)
}

func TestAPI_GetCatalog(t *testing.T) {
	srv, _ := setupTestServer(t)

	w := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/api/catalog", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
}

func TestAPI_UpdateCatalog(t *testing.T) {
	srv, database := setupTestServer(t)

	entry := core.CatalogEntry{
		ID: "cat_test", Name: "test-tool", Description: "测试工具",
		AIConfidence: 0.8, AIProvider: "offline",
	}
	database.InsertCatalogEntry(entry)

	body, _ := json.Marshal(map[string]any{
		"description": "更新描述",
	})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("PUT", "/api/catalog/cat_test", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
}

func TestAPI_GetLogs(t *testing.T) {
	srv, _ := setupTestServer(t)

	w := httptest.NewRecorder()
	req := httptest.NewRequest("GET", "/api/logs", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
}

func TestAPI_ExportCSV(t *testing.T) {
	srv, database := setupTestServer(t)

	database.InsertOperationLog(core.OperationLog{
		Timestamp: time.Now().UTC(), Operation: "DELETE",
		SourcePath: "/test/file.exe", Status: "success", SessionID: "s1",
	})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("POST", "/api/export", nil)
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
	body, _ := io.ReadAll(w.Body)
	assert.Contains(t, string(body), "timestamp,operation")
}

func TestAPI_Clean_NoFiles(t *testing.T) {
	srv, _ := setupTestServer(t)

	body, _ := json.Marshal(map[string]any{"confirm": false})

	w := httptest.NewRecorder()
	req := httptest.NewRequest("POST", "/api/clean", bytes.NewReader(body))
	req.Header.Set("Content-Type", "application/json")
	srv.Engine.ServeHTTP(w, req)

	assert.Equal(t, http.StatusOK, w.Code)
}
