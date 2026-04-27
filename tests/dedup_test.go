package tests

import (
	"testing"
	"time"

	"filesweep/internal/core"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func makeRecord(name string, hash string, size int64, modTime time.Time) core.FileRecord {
	ext := ""
	for i := len(name) - 1; i >= 0; i-- {
		if name[i] == '.' {
			ext = name[i:]
			break
		}
	}
	return core.FileRecord{
		Name:      name,
		FileHash:  hash,
		FileSize:  size,
		ModTime:   modTime,
		Extension: ext,
	}
}

func TestDedup_ExactHashMatch(t *testing.T) {
	now := time.Now()
	sameHash := "abc123def456"
	records := []core.FileRecord{
		makeRecord("python-3.11.0.exe", sameHash, 1000, now),
		makeRecord("python-3.10.8.exe", sameHash, 1000, now.Add(-1*time.Hour)),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	require.Len(t, groups, 1)
	assert.Equal(t, "hash_match", groups[0].Reason)
	assert.Equal(t, "python-3.11.0.exe", groups[0].Representative.Name)
	assert.Len(t, groups[0].Duplicates, 1)
}

func TestDedup_PreferUncompressed(t *testing.T) {
	now := time.Now()
	records := []core.FileRecord{
		makeRecord("nmap-7.94-setup.exe", "hash_exe", 2000, now),
		makeRecord("nmap-7.94.zip", "hash_zip", 2000, now),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	require.Len(t, groups, 1)
	assert.Equal(t, "nmap-7.94-setup.exe", groups[0].Representative.Name)
}

func TestDedup_VersionNewestWins(t *testing.T) {
	now := time.Now()
	records := []core.FileRecord{
		makeRecord("hutool-all-5.8.22.jar", "hash_new", 3000, now),
		makeRecord("hutool-all-5.8.18.jar", "hash_old", 3000, now.Add(-48*time.Hour)),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	require.Len(t, groups, 1)
	assert.Equal(t, "hutool-all-5.8.22.jar", groups[0].Representative.Name)
}

func TestDedup_FuzzyNameDiffSize(t *testing.T) {
	now := time.Now()
	records := []core.FileRecord{
		makeRecord("deploy_prod.sh", "hash_a", 100, now),
		makeRecord("deploy-prod.sh", "hash_b", 200, now.Add(-1*time.Hour)),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	require.Len(t, groups, 1)
	assert.Equal(t, "fuzzy_name", groups[0].Reason)
	assert.Equal(t, "deploy_prod.sh", groups[0].Representative.Name)
}

func TestDedup_SizeOnlyMatch(t *testing.T) {
	now := time.Now()
	records := []core.FileRecord{
		makeRecord("deploy_prod.sh", "hash_a", 100, now),
		makeRecord("deploy-prod.sh", "hash_b", 100, now.Add(-1*time.Hour)),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	require.Len(t, groups, 1)
	assert.Equal(t, "size_only", groups[0].Reason)
}

func TestDedup_NoDuplicates(t *testing.T) {
	now := time.Now()
	records := []core.FileRecord{
		makeRecord("nmap-setup.exe", "hash1", 100, now),
		makeRecord("python-installer.exe", "hash2", 200, now),
		makeRecord("report.pdf", "hash3", 300, now),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	assert.Empty(t, groups)
}

func TestDedup_MultipleGroups(t *testing.T) {
	now := time.Now()
	sameHash := "shared_hash"
	records := []core.FileRecord{
		makeRecord("python-3.11.0.exe", sameHash, 1000, now),
		makeRecord("python-3.10.8.exe", sameHash, 1000, now.Add(-1*time.Hour)),
		makeRecord("hutool-5.8.22.jar", "h1", 2000, now),
		makeRecord("hutool-5.8.18.jar", "h2", 2000, now.Add(-48*time.Hour)),
	}

	d := core.NewDedupDetector(true, 2)
	groups := d.Detect(records)

	assert.Len(t, groups, 2)

	reasons := make(map[string]bool)
	for _, g := range groups {
		reasons[g.Reason] = true
	}
	assert.True(t, reasons["hash_match"])
}
