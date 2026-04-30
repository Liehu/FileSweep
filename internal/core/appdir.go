package core

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"strings"
)

type AppDirSignature struct {
	IsAppDir   bool
	MainExe    string
	AppName    string
	Confidence float64
	Reason     string // exe+dll / exe+doc / single-exe+dll
}

func DetectAppDir(dirPath string) AppDirSignature {
	entries, err := os.ReadDir(dirPath)
	if err != nil {
		return AppDirSignature{}
	}

	var exes, dlls []string
	var hasDoc bool
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		name := e.Name()
		nameLower := strings.ToLower(name)
		ext := strings.ToLower(filepath.Ext(name))

		switch ext {
		case ".exe":
			if !isNoiseExe(nameLower) {
				exes = append(exes, name)
			}
		case ".dll":
			dlls = append(dlls, name)
		default:
			if !hasDoc && isDocFile(nameLower) {
				hasDoc = true
			}
		}
	}

	dirBase := filepath.Base(dirPath)

	// R1: exe + >=3 dll => confidence 0.90
	if len(exes) >= 1 && len(dlls) >= 3 {
		mainExe := pickMainExe(exes, dirBase)
		return AppDirSignature{
			IsAppDir:   true,
			MainExe:    mainExe,
			AppName:    inferAppName(dirBase),
			Confidence: 0.90,
			Reason:     "exe+dll",
		}
	}

	// R2: exe + doc feature file => confidence 0.80
	if len(exes) >= 1 && hasDoc {
		mainExe := pickMainExe(exes, dirBase)
		return AppDirSignature{
			IsAppDir:   true,
			MainExe:    mainExe,
			AppName:    inferAppName(dirBase),
			Confidence: 0.80,
			Reason:     "exe+doc",
		}
	}

	// R3: exactly 1 exe + 1~2 dll => confidence 0.70
	if len(exes) == 1 && len(dlls) >= 1 && len(dlls) <= 2 {
		return AppDirSignature{
			IsAppDir:   true,
			MainExe:    exes[0],
			AppName:    inferAppName(dirBase),
			Confidence: 0.70,
			Reason:     "single-exe+dll",
		}
	}

	return AppDirSignature{}
}

func inferAppName(dirBase string) string {
	ver, ok := ExtractVersion(dirBase)
	if !ok || ver == "" {
		return dirBase
	}
	name := dirBase
	idx := strings.Index(name, ver)
	if idx > 0 {
		name = name[:idx]
	}
	name = strings.TrimRight(name, "-_ vV.")
	if name == "" {
		return dirBase
	}
	return name
}

func pickMainExe(candidates []string, dirName string) string {
	if len(candidates) == 1 {
		return candidates[0]
	}
	dirNorm := normalizeForPick(strings.ToLower(strings.ReplaceAll(dirName, " ", "")))

	best := candidates[0]
	bestDist := levenshtein(dirNorm, normalizeForPick(strings.ToLower(strings.TrimSuffix(best, ".exe"))))
	for _, c := range candidates[1:] {
		cNorm := normalizeForPick(strings.ToLower(strings.TrimSuffix(c, ".exe")))
		d := levenshtein(dirNorm, cNorm)
		if d < bestDist {
			bestDist = d
			best = c
		}
	}
	return best
}

func normalizeForPick(s string) string {
	s = strings.ReplaceAll(s, " ", "")
	s = strings.ReplaceAll(s, "-", "")
	s = strings.ReplaceAll(s, "_", "")
	return s
}

func isNoiseExe(nameLower string) bool {
	prefixes := []string{
		"unin", "unins", "uninst", "uninstall",
		"helper", "updater", "update",
		"crashreport", "crash_report",
		"setup", "install",
		"registrator", "register",
		"elevate", "launcher_helper",
	}
	for _, p := range prefixes {
		if strings.HasPrefix(nameLower, p) {
			return true
		}
	}
	return false
}

func isDocFile(nameLower string) bool {
	docFiles := map[string]bool{
		"readme.txt": true, "readme.md": true, "readme": true,
		"license.txt": true, "license.md": true, "licence.txt": true,
		"release.txt": true, "release_notes.txt": true,
		"changelog.txt": true, "changes.txt": true,
		"说明.txt": true, "使用说明.txt": true, "使用说明.md": true, "说明书.txt": true,
		"帮助.txt": true, "帮助文档.txt": true, "版本说明.txt": true, "更新日志.txt": true,
		"readme_zh.txt": true, "readme_cn.txt": true,
	}
	return docFiles[nameLower]
}

func computeDirHash(dirPath string, exeNames []string) string {
	h := sha256.New()
	h.Write([]byte(dirPath + "|"))
	h.Write([]byte(strings.Join(exeNames, ",")))
	return hex.EncodeToString(h.Sum(nil))
}

func computeDirSize(dirPath string) int64 {
	var total int64
	filepath.WalkDir(dirPath, func(_ string, d os.DirEntry, err error) error {
		if err != nil || d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			return nil
		}
		total += info.Size()
		return nil
	})
	return total
}
