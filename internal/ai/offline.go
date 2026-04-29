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

func (o *OfflineEnricher) Enrich(ctx context.Context, req EnrichRequest, categories []string) (EnrichResult, error) {
	if o.db == nil {
		return EnrichResult{Confidence: 0, NeedsReview: true, Provider: "offline"}, nil
	}

	baseName := normalizeForMatch(req.Name)
	row := o.db.QueryRowContext(ctx,
		"SELECT description, homepage_url, download_url, latest_version, license, functional_category, tags FROM knowledge WHERE match_name = ? LIMIT 1",
		baseName,
	)

	var desc, homepage, download, latestVer, license, funcCat, tagsStr string
	if err := row.Scan(&desc, &homepage, &download, &latestVer, &license, &funcCat, &tagsStr); err != nil {
		return EnrichResult{Confidence: 0, NeedsReview: true, Provider: "offline"}, nil
	}

	var tags []string
	json.Unmarshal([]byte(tagsStr), &tags)

	return EnrichResult{
		Description:        desc,
		HomepageURL:        homepage,
		DownloadURL:        download,
		LatestVersion:      latestVer,
		License:            license,
		FunctionalCategory: funcCat,
		Tags:               tags,
		Confidence:         0.85,
		NeedsReview:        false,
		Provider:           "offline",
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
	if len(base) > 12 {
		base = base[:12]
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

	db.Exec("CREATE TABLE IF NOT EXISTS knowledge (match_name TEXT PRIMARY KEY, name TEXT, description TEXT, homepage_url TEXT, download_url TEXT, latest_version TEXT, license TEXT, functional_category TEXT DEFAULT '', tags TEXT)")
	db.Exec("DELETE FROM knowledge")

	// Patch old tables that lack functional_category column
	db.Exec("ALTER TABLE knowledge ADD COLUMN functional_category TEXT DEFAULT ''")

	for _, e := range entries {
		tags, _ := json.Marshal(e.Tags)
		matchName := normalizeForMatch(e.Name)
		db.Exec("INSERT OR REPLACE INTO knowledge (match_name, name, description, homepage_url, download_url, latest_version, license, functional_category, tags) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
			matchName, e.Name, e.Description, e.HomepageURL, e.DownloadURL, e.LatestVersion, e.License, e.FunctionalCategory, string(tags))
	}
	return nil
}

type OfflineEntry struct {
	Name               string
	Description        string
	HomepageURL        string
	DownloadURL        string
	LatestVersion      string
	License            string
	FunctionalCategory string
	Tags               []string
}

func DefaultOfflineEntries() []OfflineEntry {
	return []OfflineEntry{
		{"nmap", "开源网络扫描与安全审计工具", "https://nmap.org", "https://nmap.org/download.html", "7.95", "GPLv2", "网络安全", []string{"security", "scanner", "pentest"}},
		{"python", "通用编程语言与解释器", "https://python.org", "https://python.org/downloads/", "3.13", "PSF", "编程开发", []string{"dev", "script", "open-source"}},
		{"wireshark", "网络协议分析工具", "https://wireshark.org", "https://wireshark.org/download.html", "4.2", "GPLv2", "网络安全", []string{"security", "forensics", "open-source"}},
		{"node", "JavaScript 运行时环境", "https://nodejs.org", "https://nodejs.org/en/download/", "22.0", "MIT", "编程开发", []string{"dev", "server", "open-source"}},
		{"git", "分布式版本控制系统", "https://git-scm.com", "https://git-scm.com/downloads", "2.45", "GPLv2", "编程开发", []string{"dev", "open-source", "utility"}},
		{"vscode", "轻量级代码编辑器", "https://code.visualstudio.com", "https://code.visualstudio.com/Download", "1.89", "MIT", "编程开发", []string{"dev", "editor", "open-source"}},
		{"vlc", "跨平台多媒体播放器", "https://videolan.org/vlc", "https://videolan.org/vlc/", "3.0.21", "GPLv2", "媒体管理", []string{"media", "player", "open-source"}},
		{"7zip", "高压缩比文件压缩工具", "https://7-zip.org", "https://7-zip.org/download.html", "24.03", "LGPL", "系统增强", []string{"sysenhance", "compress", "open-source"}},
		{"putty", "SSH/Telnet 远程连接工具", "https://putty.org", "https://putty.org", "0.81", "MIT", "系统增强", []string{"sysenhance", "remote", "open-source"}},
		{"curl", "命令行 HTTP 客户端", "https://curl.se", "https://curl.se/download.html", "8.7", "MIT", "编程开发", []string{"dev", "cli", "open-source"}},
		{"hutool", "Java 工具类库", "https://hutool.cn", "https://hutool.cn/docs/#/hutool", "5.8.25", "MIT", "编程开发", []string{"dev", "utility", "open-source"}},
		{"docker", "容器化平台", "https://docker.com", "https://docker.com/products/docker-desktop/", "26.0", "Apache-2.0", "编程开发", []string{"dev", "server", "open-source"}},
		{"go", "Go 编程语言", "https://go.dev", "https://go.dev/dl/", "1.23", "BSD-3", "编程开发", []string{"dev", "open-source", "cross-platform"}},
		{"rust", "Rust 编程语言", "https://rust-lang.org", "https://rust-lang.org/tools/install", "1.78", "MIT", "编程开发", []string{"dev", "open-source", "cross-platform"}},
		{"ffmpeg", "音视频处理工具", "https://ffmpeg.org", "https://ffmpeg.org/download.html", "7.0", "LGPL", "媒体管理", []string{"media", "recorder", "open-source"}},
		{"everything", "文件名快速搜索工具", "https://voidtools.com", "https://voidtools.com/downloads/", "1.4.1", "MIT", "系统增强", []string{"sysenhance", "file-manager", "utility"}},
		{"obs", "开源直播录屏软件", "https://obsproject.com", "https://obsproject.com/download", "30.2", "GPLv2", "媒体管理", []string{"media", "recorder", "open-source"}},
		{"notepad++", "轻量级文本编辑器", "https://notepad-plus-plus.org", "https://notepad-plus-plus.org/downloads/", "8.6", "GPLv3", "编程开发", []string{"dev", "editor", "open-source"}},
		{"chrome", "Google 网页浏览器", "https://google.com/chrome", "https://google.com/chrome/", "125.0", "Proprietary", "系统增强", []string{"sysenhance", "client", "cross-platform"}},
		{"firefox", "开源网页浏览器", "https://mozilla.org/firefox", "https://mozilla.org/firefox/download/", "126.0", "MPL-2.0", "系统增强", []string{"sysenhance", "client", "open-source"}},
		{"filezilla", "跨平台FTP客户端", "https://filezilla-project.org", "https://filezilla-project.org/download.php", "3.67", "GPLv2", "系统增强", []string{"sysenhance", "client", "open-source"}},
		{"vmware", "虚拟机软件", "https://vmware.com", "https://vmware.com/products/workstation-pro.html", "17.5", "Commercial", "操作系统", []string{"os", "utility", "commercial"}},
		{"postman", "API 开发测试平台", "https://postman.com", "https://postman.com/downloads/", "11.5", "Commercial", "编程开发", []string{"dev", "open-source", "utility"}},
		{"navicat", "数据库管理与开发工具", "https://navicat.com", "https://navicat.com/download", "17.0", "Commercial", "编程开发", []string{"dev", "commercial", "utility"}},
		{"jdk", "Java 开发工具包", "https://oracle.com/java", "https://oracle.com/java/technologies/downloads/", "22.0", "Proprietary", "编程开发", []string{"dev", "installer", "cross-platform"}},
	}
}
