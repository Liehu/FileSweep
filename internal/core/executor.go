package core

import (
	"fmt"
	"io"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

type Operation string

const (
	OpMove   Operation = "MOVE"
	OpDelete Operation = "DELETE"
	OpRename Operation = "RENAME"
)

type OperationLogger interface {
	InsertOperationLog(l OperationLog) error
}

type ExecutorAction struct {
	Operation Operation
	Source    string
	Dest      string
	Reason    string
	File      FileRecord
}

type Executor struct {
	DryRun        bool
	Logger        OperationLogger
	ScanDir       string
	UseRecycleBin bool
}

func NewExecutor(dryRun bool, logger OperationLogger, scanDir string) *Executor {
	return &Executor{DryRun: dryRun, Logger: logger, ScanDir: scanDir, UseRecycleBin: true}
}

func (e *Executor) Execute(actions []ExecutorAction, sessionID string) ([]OperationLog, error) {
	var logs []OperationLog

	for _, action := range actions {
		opLog := OperationLog{
			Timestamp:  time.Now().UTC(),
			Operation:  action.Operation,
			SourcePath: action.Source,
			DestPath:   action.Dest,
			Reason:     action.Reason,
			FileHash:   action.File.FileHash,
			FileSize:   action.File.FileSize,
			SessionID:  sessionID,
			RecordID:   action.File.ID,
		}

		if e.DryRun {
			opLog.Status = "dry_run"
			opLog.CanRevert = false
			logs = append(logs, opLog)
			slog.Info("[DRY-RUN]", "operation", action.Operation, "source", action.Source)
			continue
		}

		var err error
		switch action.Operation {
		case OpMove:
			err = e.moveFile(action.Source, action.Dest)
			opLog.CanRevert = true
		case OpDelete:
			if e.UseRecycleBin {
				err = e.recycleFile(action.Source)
				opLog.CanRevert = true
			} else {
				err = e.deleteFile(action.Source)
				opLog.CanRevert = false
			}
		case OpRename:
			err = e.renameFile(action.Source, action.Dest)
			opLog.CanRevert = true
		}

		if err != nil {
			opLog.Status = "error"
			slog.Error("执行操作失败", "operation", action.Operation, "source", action.Source, "error", err)
		} else {
			opLog.Status = "success"
		}

		if e.Logger != nil {
			if logErr := e.Logger.InsertOperationLog(opLog); logErr != nil {
				slog.Error("写入操作日志失败", "error", logErr)
			}
		}

		logs = append(logs, opLog)
	}

	return logs, nil
}

func (e *Executor) moveFile(src, dst string) error {
	if err := os.MkdirAll(filepath.Dir(dst), 0755); err != nil {
		return fmt.Errorf("创建目标目录失败: %w", err)
	}
	if err := os.Rename(src, dst); err == nil {
		return nil
	}
	return copyAndRemove(src, dst)
}

func (e *Executor) recycleFile(path string) error {
	if runtime.GOOS == "windows" {
		absPath, err := filepath.Abs(path)
		if err != nil {
			return e.deleteFile(path)
		}
		// Use Microsoft.VisualBasic which is more reliable than Shell.Application
		escaped := strings.ReplaceAll(absPath, "'", "''")
		psScript := fmt.Sprintf(
			`Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('%s', 'OnlyErrorDialogs', 'SendToRecycleBin')`,
			escaped,
		)
		cmd := exec.Command("powershell", "-NoProfile", "-NonInteractive", "-Command", psScript)
		if out, err := cmd.CombinedOutput(); err != nil {
			slog.Warn("Windows recycle bin unavailable, moving to ~/.filesweep_trash", "path", absPath, "error", err, "output", string(out))
			return e.moveToTrashDir(path)
		}
		return nil
	}
	return e.moveToTrashDir(path)
}

func (e *Executor) moveToTrashDir(path string) error {
	trashDir := filepath.Join(os.Getenv("HOME"), ".filesweep_trash")
	if err := os.MkdirAll(trashDir, 0755); err != nil {
		return e.deleteFile(path)
	}
	dest := filepath.Join(trashDir, filepath.Base(path))
	if _, err := os.Stat(dest); err == nil {
		ext := filepath.Ext(path)
		base := filepath.Base(path[:len(path)-len(ext)])
		dest = filepath.Join(trashDir, fmt.Sprintf("%s_%d%s", base, time.Now().UnixMilli(), ext))
	}
	return os.Rename(path, dest)
}

func (e *Executor) deleteFile(path string) error {
	return os.Remove(path)
}

func (e *Executor) renameFile(oldPath, newPath string) error {
	return os.Rename(oldPath, newPath)
}

func copyAndRemove(src, dst string) error {
	srcF, err := os.Open(src)
	if err != nil {
		return fmt.Errorf("打开源文件失败: %w", err)
	}
	defer srcF.Close()

	dstF, err := os.Create(dst)
	if err != nil {
		return fmt.Errorf("创建目标文件失败: %w", err)
	}
	defer dstF.Close()

	if _, err := io.Copy(dstF, srcF); err != nil {
		os.Remove(dst)
		return fmt.Errorf("复制文件失败: %w", err)
	}
	dstF.Close()

	if err := os.Remove(src); err != nil {
		slog.Warn("复制完成但删除源文件失败", "src", src, "error", err)
	}
	return nil
}

func RevertMove(src, dst string) error {
	if _, err := os.Stat(dst); err != nil {
		return fmt.Errorf("目标文件不存在: %s", dst)
	}
	if err := os.MkdirAll(filepath.Dir(src), 0755); err != nil {
		return fmt.Errorf("创建源目录失败: %w", err)
	}
	if err := os.Rename(dst, src); err != nil {
		return copyAndRemove(dst, src)
	}
	return nil
}

func RevertFromTrash(trashPath, originalPath string) error {
	if _, err := os.Stat(trashPath); err != nil {
		return fmt.Errorf("回收站中文件不存在: %s", trashPath)
	}
	if err := os.MkdirAll(filepath.Dir(originalPath), 0755); err != nil {
		return fmt.Errorf("创建目标目录失败: %w", err)
	}
	if err := os.Rename(trashPath, originalPath); err != nil {
		return copyAndRemove(trashPath, originalPath)
	}
	return nil
}

func FindInTrash(originalPath string) string {
	trashDir := filepath.Join(os.Getenv("HOME"), ".filesweep_trash")
	name := filepath.Base(originalPath)
	candidate := filepath.Join(trashDir, name)
	if _, err := os.Stat(candidate); err == nil {
		return candidate
	}
	entries, err := os.ReadDir(trashDir)
	if err != nil {
		return ""
	}
	ext := filepath.Ext(name)
	base := name[:len(name)-len(ext)]
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		n := e.Name()
		if ext != "" && len(n) > len(base) && n[:len(base)] == base && n[len(n)-len(ext):] == ext {
			return filepath.Join(trashDir, n)
		}
	}
	return ""
}
