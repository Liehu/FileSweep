package core

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
	"time"
)

type Scanner struct {
	Workers       int
	ProgressCh    chan ScanProgress
	DetectAppDirs bool
}

func NewScanner() *Scanner {
	w := runtime.NumCPU()
	if w < 1 {
		w = 1
	}
	return &Scanner{
		Workers:       w,
		ProgressCh:    make(chan ScanProgress, 16),
		DetectAppDirs: true,
	}
}

type scanEntry struct {
	path       string
	info       os.FileInfo
	isAppDir   bool
	appDirPath string
	appDirSig  AppDirSignature
}

func (s *Scanner) Scan(ctx context.Context, dir string, recursive bool, detectAppDirs bool) ([]FileRecord, error) {
	absDir, err := filepath.Abs(dir)
	if err != nil {
		return nil, fmt.Errorf("解析路径失败: %w", err)
	}

	s.DetectAppDirs = detectAppDirs

	entries, err := s.walkDir(ctx, absDir, recursive)
	if err != nil {
		return nil, err
	}

	select {
	case s.ProgressCh <- ScanProgress{Total: len(entries), Stage: "hashing"}:
	default:
	}

	records := s.hashFiles(ctx, entries, absDir)

	return records, nil
}

func (s *Scanner) walkDir(ctx context.Context, dir string, recursive bool) ([]scanEntry, error) {
	var entries []scanEntry
	appDirPaths := make(map[string]bool)

	walkFn := func(path string, d os.DirEntry, err error) error {
		if err != nil {
			slog.Warn("跳过无法访问的文件", "path", path, "error", err)
			return nil
		}

		select {
		case <-ctx.Done():
			return ctx.Err()
		default:
		}

		// #5: 跳过隐藏文件和目录（.开头）
		baseName := d.Name()
		if strings.HasPrefix(baseName, ".") {
			if d.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}

		if d.IsDir() {
			if !recursive && path != dir {
				return filepath.SkipDir
			}
			// AppDir detection: check non-root subdirectories
			if s.DetectAppDirs && path != dir {
				sig := DetectAppDir(path)
				if sig.IsAppDir {
					info, err := d.Info()
					if err != nil {
						return nil
					}
					appDirPaths[path] = true
					entries = append(entries, scanEntry{
						path:       path,
						info:       info,
						isAppDir:   true,
						appDirPath: path,
						appDirSig:  sig,
					})
					select {
					case s.ProgressCh <- ScanProgress{
						Total:       0,
						Done:        len(entries),
						CurrentFile: "[软件目录] " + sig.AppName,
						Stage:       "walking",
					}:
					default:
					}
					return filepath.SkipDir
				}
			}
			return nil
		}

		// Skip files inside detected app dirs
		parentDir := filepath.Dir(path)
		if appDirPaths[parentDir] {
			return nil
		}

		if d.Type() == os.ModeSymlink {
			return nil
		}

		info, err := d.Info()
		if err != nil {
			slog.Warn("获取文件信息失败", "path", path, "error", err)
			return nil
		}

		// #9: walk 阶段 Total 传 0 表示未知总数
		select {
		case s.ProgressCh <- ScanProgress{
			Total:       0,
			Done:        len(entries) + 1,
			CurrentFile: baseName,
			Stage:       "walking",
		}:
		default:
		}

		entries = append(entries, scanEntry{path: path, info: info})
		return nil
	}

	if err := filepath.WalkDir(dir, walkFn); err != nil && err != context.Canceled {
		return nil, fmt.Errorf("遍历目录失败: %w", err)
	}

	return entries, nil
}

func (s *Scanner) hashFiles(ctx context.Context, entries []scanEntry, baseDir string) []FileRecord {
	var (
		mu      sync.Mutex
		records []FileRecord
		wg      sync.WaitGroup
		sem     = make(chan struct{}, s.Workers)
		done    int
	)

	for _, entry := range entries {
		select {
		case <-ctx.Done():
			return records
		default:
		}

		wg.Add(1)
		sem <- struct{}{}

		go func(e scanEntry) {
			defer wg.Done()
			defer func() { <-sem }()

			var record FileRecord

			if e.isAppDir {
				sig := e.appDirSig
				dirBase := filepath.Base(e.appDirPath)

				childEntries, _ := os.ReadDir(e.appDirPath)
				var exeNames []string
				for _, ce := range childEntries {
					if !ce.IsDir() && strings.ToLower(filepath.Ext(ce.Name())) == ".exe" {
						exeNames = append(exeNames, ce.Name())
					}
				}
				hash := computeDirHash(e.appDirPath, exeNames)
				size := computeDirSize(e.appDirPath)
				ver, _ := ExtractVersion(dirBase)
				mainExePath := filepath.Join(e.appDirPath, sig.MainExe)

				record = FileRecord{
					ID:           NewFileRecordIDWithPath(hash, e.appDirPath),
					Name:         sig.AppName,
					Version:      ver,
					LocalPath:    mainExePath,
					FileSize:     size,
					FileHash:     hash,
					Extension:    ".exe",
					Status:       "active",
					ScannedAt:    time.Now().UTC(),
					ModTime:      e.info.ModTime(),
					IsAppDir:     true,
					AppDirPath:   e.appDirPath,
					AppDirReason: sig.Reason,
				}
			} else {
				hash, err := computeHash(e.path)
				if err != nil {
					slog.Warn("计算哈希失败", "path", e.path, "error", err)
					return
				}

				name := filepath.Base(e.path)
				ext := filepath.Ext(name)
				ver, _ := ExtractVersion(name)

				record = FileRecord{
					ID:        NewFileRecordIDWithPath(hash, e.path),
					Name:      name,
					Version:   ver,
					LocalPath: e.path,
					FileSize:  e.info.Size(),
					FileHash:  hash,
					Extension: ext,
					Status:    "active",
					ScannedAt: time.Now().UTC(),
					ModTime:   e.info.ModTime(),
				}
			}

			mu.Lock()
			records = append(records, record)
			done++
			currentDone := done
			mu.Unlock()

			progressName := record.Name
			if record.IsAppDir {
				progressName = "[目录] " + record.Name
			}
			// #7: 在锁外发送进度，避免 channel 满时死锁
			select {
			case s.ProgressCh <- ScanProgress{
				Total:       len(entries),
				Done:        currentDone,
				CurrentFile: progressName,
				Stage:       "hashing",
			}:
			default:
			}
		}(entry)
	}

	wg.Wait()
	return records
}

func computeHash(path string) (string, error) {
	f, err := os.Open(path)
	if err != nil {
		return "", fmt.Errorf("打开文件失败: %w", err)
	}
	defer f.Close()

	h := sha256.New()
	if _, err := io.Copy(h, f); err != nil {
		return "", fmt.Errorf("读取文件失败: %w", err)
	}

	return hex.EncodeToString(h.Sum(nil)), nil
}
