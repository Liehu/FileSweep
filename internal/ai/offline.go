package ai

import (
	"context"
	"database/sql"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	_ "modernc.org/sqlite"
)

type OfflineEnricher struct {
	db *sql.DB
}

func NewOfflineEnricher(dbPath string) (*OfflineEnricher, error) {
	if _, err := os.Stat(dbPath); os.IsNotExist(err) {
		return &OfflineEnricher{db: nil}, nil
	}

	db, err := sql.Open("sqlite", dbPath)
	if err != nil {
		return nil, fmt.Errorf("打开离线知识库失败: %w", err)
	}
	return &OfflineEnricher{db: db}, nil
}

func (o *OfflineEnricher) Close() error {
	if o.db != nil {
		return o.db.Close()
	}
	return nil
}

func (o *OfflineEnricher) Name() string {
	return "offline"
}

func (o *OfflineEnricher) Enrich(ctx context.Context, req EnrichRequest) (EnrichResult, error) {
	if o.db == nil {
		return EnrichResult{Confidence: 0, NeedsReview: true, Provider: "offline"}, nil
	}

	baseName := normalizeForMatch(req.Name)
	row := o.db.QueryRowContext(ctx,
		"SELECT description, homepage_url, download_url, latest_version, license, tags FROM knowledge WHERE match_name = ? LIMIT 1",
		baseName,
	)

	var desc, homepage, download, latestVer, license, tagsStr string
	if err := row.Scan(&desc, &homepage, &download, &latestVer, &license, &tagsStr); err != nil {
		return EnrichResult{Confidence: 0, NeedsReview: true, Provider: "offline"}, nil
	}

	var tags []string
	json.Unmarshal([]byte(tagsStr), &tags)

	return EnrichResult{
		Description:   desc,
		HomepageURL:   homepage,
		DownloadURL:   download,
		LatestVersion: latestVer,
		License:       license,
		Tags:          tags,
		Confidence:    0.85,
		NeedsReview:   false,
		Provider:      "offline",
	}, nil
}

func normalizeForMatch(name string) string {
	base := strings.ToLower(name)
	for _, ext := range []string{".exe", ".msi", ".dmg", ".pkg", ".deb", ".rpm", ".zip", ".7z", ".rar", ".gz", ".tar", ".jar", ".py", ".sh", ".bat", ".cmd", ".pdf", ".docx", ".txt", ".md"} {
		base = strings.TrimSuffix(base, ext)
	}
	for _, sep := range []string{"-", "_", ".", " "} {
		base = strings.ReplaceAll(base, sep, "")
	}
	for _, suffix := range []string{"setup", "install", "installer", "win64", "win32", "amd64", "x64", "x86", "64bit", "32bit"} {
		base = strings.TrimSuffix(base, suffix)
	}
	if len(base) > 8 {
		base = base[:8]
	}
	return base
}

func CreateOfflineDB(dbPath string, entries []OfflineEntry) error {
	dir := filepath.Dir(dbPath)
	os.MkdirAll(dir, 0755)

	db, err := sql.Open("sqlite", dbPath)
	if err != nil {
		return err
	}
	defer db.Close()

	db.Exec("CREATE TABLE IF NOT EXISTS knowledge (match_name TEXT PRIMARY KEY, name TEXT, description TEXT, homepage_url TEXT, download_url TEXT, latest_version TEXT, license TEXT, tags TEXT)")
	db.Exec("DELETE FROM knowledge")

	for _, e := range entries {
		tags, _ := json.Marshal(e.Tags)
		matchName := normalizeForMatch(e.Name)
		db.Exec("INSERT OR REPLACE INTO knowledge (match_name, name, description, homepage_url, download_url, latest_version, license, tags) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
			matchName, e.Name, e.Description, e.HomepageURL, e.DownloadURL, e.LatestVersion, e.License, string(tags))
	}
	return nil
}

type OfflineEntry struct {
	Name         string
	Description  string
	HomepageURL  string
	DownloadURL  string
	LatestVersion string
	License      string
	Tags         []string
}

func DefaultOfflineEntries() []OfflineEntry {
	return []OfflineEntry{
		{"nmap", "开源网络扫描与安全审计工具", "https://nmap.org", "https://nmap.org/download.html", "7.95", "GPLv2", []string{"network", "security", "scanner"}},
		{"python", "通用编程语言与解释器", "https://python.org", "https://python.org/downloads/", "3.13", "PSF", []string{"language", "programming", "scripting"}},
		{"wireshark", "网络协议分析工具", "https://wireshark.org", "https://wireshark.org/download.html", "4.2", "GPLv2", []string{"network", "analyzer", "capture"}},
		{"node", "JavaScript 运行时环境", "https://nodejs.org", "https://nodejs.org/en/download/", "22.0", "MIT", []string{"runtime", "javascript", "server"}},
		{"git", "分布式版本控制系统", "https://git-scm.com", "https://git-scm.com/downloads", "2.45", "GPLv2", []string{"vcs", "scm", "devtools"}},
		{"vscode", "轻量级代码编辑器", "https://code.visualstudio.com", "https://code.visualstudio.com/Download", "1.89", "MIT", []string{"editor", "ide", "microsoft"}},
		{"vlc", "跨平台多媒体播放器", "https://videolan.org/vlc", "https://videolan.org/vlc/", "3.0.21", "GPLv2", []string{"media", "player", "video"}},
		{"7zip", "高压缩比文件压缩工具", "https://7-zip.org", "https://7-zip.org/download.html", "24.03", "LGPL", []string{"archive", "compression", "utility"}},
		{"putty", "SSH/Telnet 远程连接工具", "https://putty.org", "https://putty.org", "0.81", "MIT", []string{"ssh", "terminal", "remote"}},
		{"curl", "命令行 HTTP 客户端", "https://curl.se", "https://curl.se/download.html", "8.7", "MIT", []string{"http", "cli", "transfer"}},
		{"hutool", "Java 工具类库", "https://hutool.cn", "https://hutool.cn/docs/#/hutool", "5.8.25", "MIT", []string{"java", "utility", "library"}},
		{"docker", "容器化平台", "https://docker.com", "https://docker.com/products/docker-desktop/", "26.0", "Apache-2.0", []string{"container", "devops", "virtualization"}},
		{"go", "Go 编程语言", "https://go.dev", "https://go.dev/dl/", "1.23", "BSD-3", []string{"language", "programming", "google"}},
		{"rust", "Rust 编程语言", "https://rust-lang.org", "https://rust-lang.org/tools/install", "1.78", "MIT", []string{"language", "programming", "systems"}},
		{"ffmpeg", "音视频处理工具", "https://ffmpeg.org", "https://ffmpeg.org/download.html", "7.0", "LGPL", []string{"media", "video", "audio"}},
	}
}
