package tests

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"filesweep/internal/ai"
	"filesweep/internal/core"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOfflineEnricher(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "offline.db")
	entries := []ai.OfflineEntry{
		{Name: "nmap-7.94-setup.exe", Description: "网络扫描工具", HomepageURL: "https://nmap.org",
			DownloadURL: "https://nmap.org/download.html", LatestVersion: "7.95",
			License: "GPLv2", Tags: []string{"network", "security"}},
		{Name: "python-3.11.exe", Description: "编程语言", HomepageURL: "https://python.org",
			LatestVersion: "3.13", License: "PSF", Tags: []string{"language"}},
	}
	require.NoError(t, ai.CreateOfflineDB(dbPath, entries))

	enricher, err := ai.NewOfflineEnricher(dbPath)
	require.NoError(t, err)
	defer enricher.Close()

	result, err := enricher.Enrich(context.Background(), ai.EnrichRequest{
		Name: "nmap-7.94-setup.exe", Extension: ".exe",
	}, nil)
	require.NoError(t, err)
	assert.Equal(t, "offline", result.Provider)
	assert.Equal(t, "网络扫描工具", result.Description)
	assert.Equal(t, "https://nmap.org", result.HomepageURL)
	assert.True(t, result.Confidence >= 0.8)
	assert.False(t, result.NeedsReview)
}

func TestOfflineEnricher_Miss(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "offline.db")
	ai.CreateOfflineDB(dbPath, ai.DefaultOfflineEntries())

	enricher, err := ai.NewOfflineEnricher(dbPath)
	require.NoError(t, err)
	defer enricher.Close()

	result, err := enricher.Enrich(context.Background(), ai.EnrichRequest{
		Name: "totally-unknown-tool-xyz.exe", Extension: ".exe",
	}, nil)
	require.NoError(t, err)
	assert.Equal(t, float64(0), result.Confidence)
	assert.True(t, result.NeedsReview)
}

func TestOfflineEnricher_NoDB(t *testing.T) {
	enricher, err := ai.NewOfflineEnricher("/nonexistent/path.db")
	require.NoError(t, err)
	defer enricher.Close()

	result, err := enricher.Enrich(context.Background(), ai.EnrichRequest{Name: "test.exe"}, nil)
	require.NoError(t, err)
	assert.Equal(t, float64(0), result.Confidence)
}

func TestOfflineEnricher_DefaultEntries(t *testing.T) {
	entries := ai.DefaultOfflineEntries()
	assert.GreaterOrEqual(t, len(entries), 10)

	for _, e := range entries {
		assert.NotEmpty(t, e.Name)
		assert.NotEmpty(t, e.Description)
	}
}

func TestParseEnrichResponse(t *testing.T) {
	jsonData := `{
		"description": "开源网络扫描与安全审计工具",
		"homepage_url": "https://nmap.org",
		"download_url": "https://nmap.org/download.html",
		"latest_version": "7.95",
		"license": "GPLv2",
		"tags": ["network", "security"],
		"confidence": 0.97
	}`

	result, err := ai.ParseEnrichResponse([]byte(jsonData), "openai")
	require.NoError(t, err)
	assert.Equal(t, "开源网络扫描与安全审计工具", result.Description)
	assert.Equal(t, "https://nmap.org", result.HomepageURL)
	assert.InDelta(t, 0.97, result.Confidence, 0.01)
	assert.False(t, result.NeedsReview)
	assert.Equal(t, "openai", result.Provider)
}

func TestParseEnrichResponse_LowConfidence(t *testing.T) {
	jsonData := `{"description": "unknown", "confidence": 0.3}`
	result, err := ai.ParseEnrichResponse([]byte(jsonData), "ollama")
	require.NoError(t, err)
	assert.True(t, result.NeedsReview)
}

func TestParseEnrichResponse_InvalidJSON(t *testing.T) {
	_, err := ai.ParseEnrichResponse([]byte("not json"), "test")
	assert.Error(t, err)
}

func TestBatchEnrich(t *testing.T) {
	enricher := &mockEnricher{results: map[string]ai.EnrichResult{
		"tool.exe": {Description: "A tool", Confidence: 0.9, Provider: "mock"},
	}}

	requests := []ai.EnrichRequest{
		{Name: "tool.exe"}, {Name: "other.exe"}, {Name: "third.exe"},
	}

	progressCh := make(chan ai.EnrichProgress, 16)
	go func() {
		for range progressCh {
		}
	}()

	results, err := ai.BatchEnrich(context.Background(), enricher, requests, nil, 2, progressCh)
	require.NoError(t, err)
	assert.Len(t, results, 3)
}

func TestPrivacyChecker(t *testing.T) {
	checker := core.NewPrivacyChecker([]string{"internal_*", "*_secret.*", "confidential/**"})

	tests := []struct {
		name     string
		path     string
		expected bool
	}{
		{"internal_tool.exe", "internal_tool.exe", true},
		{"data_secret.txt", "data_secret.txt", true},
		{"normal.exe", "normal.exe", false},
		{"deploy.sh", "deploy.sh", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			record := core.FileRecord{Name: tt.name, LocalPath: tt.path}
			assert.Equal(t, tt.expected, checker.ShouldSkip(record))
		})
	}
}

func TestPrivacyChecker_AISkip(t *testing.T) {
	checker := core.NewPrivacyChecker(nil)
	record := core.FileRecord{Name: "anything.exe", AISkip: true}
	assert.True(t, checker.ShouldSkip(record))
}

func TestCreateOfflineDB(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test_offline.db")
	entries := ai.DefaultOfflineEntries()

	err := ai.CreateOfflineDB(dbPath, entries)
	require.NoError(t, err)
	assert.FileExists(t, dbPath)

	info, _ := os.Stat(dbPath)
	assert.Greater(t, info.Size(), int64(0))
}

type mockEnricher struct {
	results map[string]ai.EnrichResult
}

func (m *mockEnricher) Enrich(ctx context.Context, req ai.EnrichRequest, categories []string) (ai.EnrichResult, error) {
	if r, ok := m.results[req.Name]; ok {
		return r, nil
	}
	return ai.EnrichResult{Confidence: 0.5, Provider: "mock", NeedsReview: true}, nil
}

func (m *mockEnricher) Name() string { return "mock" }

// Export for testing
func TestMain(m *testing.M) {
	os.Exit(m.Run())
}

// Re-export for test convenience
var _ = json.Marshal
var _ = time.Now
