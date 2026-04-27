package tests

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"filesweep/internal/core"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestExecutor_DryRun(t *testing.T) {
	dir := t.TempDir()
	srcFile := filepath.Join(dir, "test.txt")
	require.NoError(t, os.WriteFile(srcFile, []byte("test"), 0644))

	executor := core.NewExecutor(true, nil, dir)
	actions := []core.ExecutorAction{
		{
			Operation: core.OpDelete,
			Source:    srcFile,
			Reason:    "test",
			File:      core.FileRecord{Name: "test.txt", FileHash: "abc", FileSize: 4},
		},
	}

	logs, err := executor.Execute(actions, "session1")
	require.NoError(t, err)
	require.Len(t, logs, 1)

	assert.Equal(t, "dry_run", logs[0].Status)
	assert.Equal(t, core.Operation("DELETE"), logs[0].Operation)

	_, err = os.Stat(srcFile)
	assert.NoError(t, err, "dry-run 模式不应删除文件")
}

func TestExecutor_MoveFile(t *testing.T) {
	dir := t.TempDir()
	srcFile := filepath.Join(dir, "source.txt")
	dstFile := filepath.Join(dir, "dest", "source.txt")
	require.NoError(t, os.WriteFile(srcFile, []byte("content"), 0644))

	executor := core.NewExecutor(false, nil, dir)
	actions := []core.ExecutorAction{
		{
			Operation: core.OpMove,
			Source:    srcFile,
			Dest:      dstFile,
			Reason:    "classify",
			File:      core.FileRecord{Name: "source.txt", FileHash: "h1", FileSize: 7},
		},
	}

	logs, err := executor.Execute(actions, "session1")
	require.NoError(t, err)
	require.Len(t, logs, 1)

	assert.Equal(t, "success", logs[0].Status)
	assert.True(t, logs[0].CanRevert)

	_, err = os.Stat(srcFile)
	assert.True(t, os.IsNotExist(err), "源文件应已移动")

	data, err := os.ReadFile(dstFile)
	require.NoError(t, err)
	assert.Equal(t, "content", string(data))
}

func TestExecutor_DeleteFile(t *testing.T) {
	dir := t.TempDir()
	srcFile := filepath.Join(dir, "delete_me.txt")
	require.NoError(t, os.WriteFile(srcFile, []byte("bye"), 0644))

	executor := core.NewExecutor(false, nil, dir)
	actions := []core.ExecutorAction{
		{
			Operation: core.OpDelete,
			Source:    srcFile,
			Reason:    "duplicate",
			File:      core.FileRecord{Name: "delete_me.txt", FileHash: "h2", FileSize: 3},
		},
	}

	logs, err := executor.Execute(actions, "session1")
	require.NoError(t, err)
	assert.Equal(t, "success", logs[0].Status)
}

func TestExecutor_RenameFile(t *testing.T) {
	dir := t.TempDir()
	oldFile := filepath.Join(dir, "old.txt")
	newFile := filepath.Join(dir, "new.txt")
	require.NoError(t, os.WriteFile(oldFile, []byte("renamed"), 0644))

	executor := core.NewExecutor(false, nil, dir)
	actions := []core.ExecutorAction{
		{
			Operation: core.OpRename,
			Source:    oldFile,
			Dest:      newFile,
			Reason:    "normalize",
			File:      core.FileRecord{Name: "old.txt", FileHash: "h3", FileSize: 7},
		},
	}

	logs, err := executor.Execute(actions, "session1")
	require.NoError(t, err)
	assert.Equal(t, "success", logs[0].Status)

	data, err := os.ReadFile(newFile)
	require.NoError(t, err)
	assert.Equal(t, "renamed", string(data))
}

func TestExportCSV(t *testing.T) {
	dir := t.TempDir()
	outputPath := filepath.Join(dir, "export.csv")

	logs := []core.OperationLog{
		{
			Timestamp:  time.Now().UTC(),
			Operation:  "MOVE",
			SourcePath: "/a.txt",
			DestPath:   "/b/a.txt",
			Reason:     "classify",
			FileHash:   "h1",
			FileSize:   100,
			Status:     "success",
			SessionID:  "s1",
			CanRevert:  true,
		},
	}

	err := core.ExportCSV(logs, outputPath)
	require.NoError(t, err)

	data, err := os.ReadFile(outputPath)
	require.NoError(t, err)
	assert.Contains(t, string(data), "timestamp,operation,source_path")
	assert.Contains(t, string(data), "MOVE")
	assert.Contains(t, string(data), "/a.txt")
}
