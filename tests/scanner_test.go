package tests

import (
	"context"
	"os"
	"path/filepath"
	"fmt"
	"runtime"
	"testing"

	"filesweep/internal/core"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func createTestFiles(t *testing.T, dir string) {
	t.Helper()

	files := map[string]string{
		"hello.txt":           "hello world",
		"test.exe":            "fake exe content",
		"subdir/nested.py":    "print('hello')",
		"subdir/deep/file.jar": "jar content",
	}

	for path, content := range files {
		fullPath := filepath.Join(dir, path)
		require.NoError(t, os.MkdirAll(filepath.Dir(fullPath), 0755))
		require.NoError(t, os.WriteFile(fullPath, []byte(content), 0644))
	}
}

func TestScanner_NonRecursive(t *testing.T) {
	dir := t.TempDir()
	createTestFiles(t, dir)

	scanner := core.NewScanner()
	records, err := scanner.Scan(context.Background(), dir, false)

	require.NoError(t, err)
	assert.Equal(t, 2, len(records), "非递归模式应只扫描顶层文件")

	names := make(map[string]bool)
	for _, r := range records {
		names[r.Name] = true
		assert.Equal(t, "active", r.Status)
		assert.NotEmpty(t, r.FileHash)
		assert.True(t, r.Extension == ".txt" || r.Extension == ".exe")
	}
	assert.True(t, names["hello.txt"])
	assert.True(t, names["test.exe"])
}

func TestScanner_Recursive(t *testing.T) {
	dir := t.TempDir()
	createTestFiles(t, dir)

	scanner := core.NewScanner()
	records, err := scanner.Scan(context.Background(), dir, true)

	require.NoError(t, err)
	assert.Equal(t, 4, len(records), "递归模式应扫描所有文件")
}

func TestScanner_HashCorrectness(t *testing.T) {
	dir := t.TempDir()
	content := "known content for hash test"
	require.NoError(t, os.WriteFile(filepath.Join(dir, "known.txt"), []byte(content), 0644))

	scanner := core.NewScanner()
	records, err := scanner.Scan(context.Background(), dir, false)

	require.NoError(t, err)
	require.Len(t, records, 1)

	expected := core.FileHashFromBytes([]byte(content))
	assert.Equal(t, expected, records[0].FileHash)
}

func TestScanner_IDGeneration(t *testing.T) {
	dir := t.TempDir()
	require.NoError(t, os.WriteFile(filepath.Join(dir, "test.txt"), []byte("x"), 0644))

	scanner := core.NewScanner()
	records, err := scanner.Scan(context.Background(), dir, false)

	require.NoError(t, err)
	require.Len(t, records, 1)
	assert.Contains(t, records[0].ID, "rec_")
}

func TestScanner_EmptyDirectory(t *testing.T) {
	dir := t.TempDir()

	scanner := core.NewScanner()
	records, err := scanner.Scan(context.Background(), dir, false)

	require.NoError(t, err)
	assert.Empty(t, records)
}

func TestScanner_Cancellation(t *testing.T) {
	dir := t.TempDir()
	for i := 0; i < 50; i++ {
		content := make([]byte, 1024)
		require.NoError(t, os.WriteFile(filepath.Join(dir, fmt.Sprintf("file_%d.txt", i)), content, 0644))
	}

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	scanner := core.NewScanner()
	records, _ := scanner.Scan(ctx, dir, false)

	assert.Empty(t, records)
}

func TestScanner_ProgressChannel(t *testing.T) {
	dir := t.TempDir()
	for i := 0; i < 5; i++ {
		require.NoError(t, os.WriteFile(filepath.Join(dir, fmt.Sprintf("file_%d.txt", i)), []byte("x"), 0644))
	}

	scanner := core.NewScanner()

	var progresses []core.ScanProgress
	done := make(chan struct{})
	go func() {
		for p := range scanner.ProgressCh {
			progresses = append(progresses, p)
		}
		close(done)
	}()

	_, err := scanner.Scan(context.Background(), dir, false)
	require.NoError(t, err)
	close(scanner.ProgressCh)
	<-done

	assert.NotEmpty(t, progresses)

	var hashProgresses []core.ScanProgress
	for _, p := range progresses {
		if p.Stage == "hashing" {
			hashProgresses = append(hashProgresses, p)
		}
	}
	assert.NotEmpty(t, hashProgresses)
}

func TestScanner_SymlinkSkip(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("跳过 Windows 符号链接测试")
	}

	dir := t.TempDir()
	target := filepath.Join(dir, "real.txt")
	require.NoError(t, os.WriteFile(target, []byte("real"), 0644))
	require.NoError(t, os.Symlink(target, filepath.Join(dir, "link.txt")))

	scanner := core.NewScanner()
	records, err := scanner.Scan(context.Background(), dir, false)

	require.NoError(t, err)
	assert.Len(t, records, 1)
	assert.Equal(t, "real.txt", records[0].Name)
}
