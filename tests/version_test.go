package tests

import (
	"os"
	"testing"
	"time"

	"filesweep/internal/core"

	"github.com/stretchr/testify/assert"
)

func TestExtractVersion(t *testing.T) {
	tests := []struct {
		filename string
		expected string
		ok       bool
	}{
		{"python-3.11.0-amd64.exe", "3.11.0", true},
		{"Python_3.10.8_win64.exe", "3.10.8", true},
		{"nmap-7.94-setup.exe", "7.94", true},
		{"hutool-all-5.8.22.jar", "5.8.22", true},
		{"vlc-3.0.20-win64.exe", "3.0.20", true},
		{"app_v2.1_release.zip", "2.1", true},
		{"tool-20240115.tar.gz", "20240115", true},
		{"deploy_prod.sh", "", false},
		{"readme.md", "", false},
		{"backup_20240101.py", "20240101", true},
		{"app_v3_setup.exe", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.filename, func(t *testing.T) {
			v, ok := core.ExtractVersion(tt.filename)
			assert.Equal(t, tt.ok, ok, "ok mismatch for %s", tt.filename)
			assert.Equal(t, tt.expected, v, "version mismatch for %s", tt.filename)
		})
	}
}

func TestCompareVersions(t *testing.T) {
	tests := []struct {
		a, b     string
		expected int
	}{
		{"3.11.0", "3.10.8", 1},
		{"3.10.0", "3.11.0", -1},
		{"5.8.22", "5.8.22", 0},
		{"7.94", "7.95", -1},
		{"20240115", "20240120", -1},
		{"20240120", "20240115", 1},
		{"2.1", "2.1", 0},
	}

	for _, tt := range tests {
		t.Run(tt.a+"_"+tt.b, func(t *testing.T) {
			result := core.CompareVersions(tt.a, tt.b)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestFindLatest(t *testing.T) {
	now := time.Now()

	t.Run("with versions", func(t *testing.T) {
		files := []core.FileRecord{
			{Name: "python-3.10.8.exe", ModTime: now},
			{Name: "python-3.11.0.exe", ModTime: now},
		}
		result := core.FindLatest(files)
		assert.Equal(t, "python-3.11.0.exe", result.Name)
	})

	t.Run("without versions", func(t *testing.T) {
		files := []core.FileRecord{
			{Name: "deploy_prod.sh", ModTime: now.Add(-1 * time.Hour)},
			{Name: "deploy-prod.sh", ModTime: now},
		}
		result := core.FindLatest(files)
		assert.Equal(t, "deploy-prod.sh", result.Name)
	})

	t.Run("single file", func(t *testing.T) {
		files := []core.FileRecord{
			{Name: "only.txt", ModTime: now},
		}
		result := core.FindLatest(files)
		assert.Equal(t, "only.txt", result.Name)
	})

	t.Run("empty returns zero value", func(t *testing.T) {
		result := core.FindLatest(nil)
		assert.Equal(t, core.FileRecord{}, result)
	})
}

func TestFindLatest_WithRealFiles(t *testing.T) {
	dir := t.TempDir()
	now := time.Now()

	files := []struct {
		name    string
		content string
		modTime time.Time
	}{
		{"hutool-all-5.8.18.jar", "old", now.Add(-48 * time.Hour)},
		{"hutool-all-5.8.22.jar", "new", now},
	}

	var records []core.FileRecord
	for _, f := range files {
		path := dir + "/" + f.name
		os.WriteFile(path, []byte(f.content), 0644)
		os.Chtimes(path, f.modTime, f.modTime)
		info, _ := os.Stat(path)
		records = append(records, core.FileRecord{
			Name:    f.name,
			ModTime: info.ModTime(),
		})
	}

	result := core.FindLatest(records)
	assert.Equal(t, "hutool-all-5.8.22.jar", result.Name)
}
