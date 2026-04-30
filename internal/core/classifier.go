package core

import (
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"gopkg.in/yaml.v3"
)

type CategoryRule struct {
	Name         string   `yaml:"name" json:"name"`
	TargetPath   string   `yaml:"target_path" json:"target_path"`
	Extensions   []string `yaml:"extensions" json:"extensions"`
	NameKeywords []string `yaml:"name_keywords" json:"name_keywords"`
}

type RulesConfig struct {
	Categories []CategoryRule `yaml:"categories"`
}

type Classifier struct {
	Rules RulesConfig
}

func NewClassifier(rulesPath string) (*Classifier, error) {
	data, err := os.ReadFile(rulesPath)
	if err != nil {
		return nil, fmt.Errorf("读取规则文件失败: %w", err)
	}
	var cfg RulesConfig
	if err := yaml.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("解析规则文件失败: %w", err)
	}
	return &Classifier{Rules: cfg}, nil
}

func NewClassifierWithRules(rules RulesConfig) *Classifier {
	return &Classifier{Rules: rules}
}

func DefaultRules() RulesConfig {
	return RulesConfig{
		Categories: []CategoryRule{
			{
				Name: "安装包", TargetPath: "Installers",
				Extensions:   []string{".exe", ".msi", ".pkg", ".dmg", ".deb", ".rpm", ".AppImage"},
				NameKeywords: []string{"setup", "install", "installer", "update"},
			},
			{
				Name: "文档", TargetPath: "Docs",
				Extensions: []string{".pdf", ".docx", ".doc", ".xls", ".xlsx", ".ppt", ".pptx", ".md", ".txt", ".epub"},
			},
			{
				Name: "压缩包", TargetPath: "Archives",
				Extensions: []string{".zip", ".7z", ".rar", ".gz", ".tar", ".xz", ".bz2", ".tar.gz", ".tar.xz", ".tar.bz2"},
			},
			{
				Name: "脚本", TargetPath: "Scripts",
				Extensions: []string{".sh", ".bash", ".py", ".bat", ".cmd", ".ps1", ".rb", ".pl"},
			},
			{
				Name: "Java工具", TargetPath: "Jars",
				Extensions: []string{".jar", ".war"},
			},
			{
				Name: "镜像", TargetPath: "Images",
				Extensions: []string{".iso", ".img", ".vmdk", ".vhd"},
			},
			{
				Name: "视频", TargetPath: "Videos",
				Extensions: []string{".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv"},
			},
			{
				Name: "音频", TargetPath: "Audio",
				Extensions: []string{".mp3", ".flac", ".wav", ".aac", ".ogg", ".wma"},
			},
			{
				Name: "图片", TargetPath: "Pictures",
				Extensions: []string{".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg"},
			},
		},
	}
}

func NewClassifierWithDefaults() *Classifier {
	return NewClassifierWithRules(DefaultRules())
}

func SaveRules(rulesPath string, cfg RulesConfig) error {
	data, err := yaml.Marshal(cfg)
	if err != nil {
		return fmt.Errorf("序列化规则失败: %w", err)
	}
	return os.WriteFile(rulesPath, data, 0644)
}

// Classify sorts rules so that deeper rules (more \ separators) are tried first,
// then falls back to top-level rules.
func (c *Classifier) Classify(file FileRecord) ClassifyResult {
	rules := c.sortedRules()
	var baseResult ClassifyResult
	found := false

	for _, rule := range rules {
		if matchExtension(rule.Extensions, file.Extension) || matchKeywords(rule.NameKeywords, file.Name) {
			baseResult = ClassifyResult{Category: rule.Name, TargetDir: rule.TargetPath}
			found = true
			break
		}
	}

	if !found {
		baseResult = ClassifyResult{Category: "未分类", TargetDir: "Uncategorized"}
	}

	return baseResult
}

// sortedRules returns rules sorted by depth (deepest first) so
// "安装包\开发工具" matches before "安装包".
func (c *Classifier) sortedRules() []CategoryRule {
	rules := make([]CategoryRule, len(c.Rules.Categories))
	copy(rules, c.Rules.Categories)
	sort.SliceStable(rules, func(i, j int) bool {
		di := strings.Count(rules[i].Name, "\\")
		dj := strings.Count(rules[j].Name, "\\")
		return di > dj
	})
	return rules
}

func matchExtension(extensions []string, ext string) bool {
	lower := strings.ToLower(ext)
	for _, e := range extensions {
		if strings.ToLower(e) == lower {
			return true
		}
	}
	return false
}

func matchKeywords(keywords []string, name string) bool {
	lower := strings.ToLower(name)
	for _, kw := range keywords {
		if strings.Contains(lower, strings.ToLower(kw)) {
			return true
		}
	}
	return false
}

func (c *Classifier) IsRedundantArchive(file FileRecord, allFiles []FileRecord) bool {
	archiveExts := map[string]bool{".zip": true, ".7z": true, ".rar": true, ".gz": true, ".tar": true}
	ext := strings.ToLower(file.Extension)
	isTarGz := strings.HasSuffix(strings.ToLower(file.Name), ".tar.gz") || strings.HasSuffix(strings.ToLower(file.Name), ".tar.bz2") || strings.HasSuffix(strings.ToLower(file.Name), ".tar.xz")
	if !archiveExts[ext] && !isTarGz {
		return false
	}

	normalized := normalizeArchiveName(file.Name)
	for _, other := range allFiles {
		if other.ID == file.ID {
			continue
		}
		if archiveExts[strings.ToLower(other.Extension)] {
			continue
		}
		if normalizeArchiveName(other.Name) == normalized {
			return true
		}
		if other.Extension != "" {
			otherBase := strings.TrimSuffix(other.Name, other.Extension)
			if normalizeArchiveName(otherBase) == normalized {
				return true
			}
		}
	}
	return false
}

func normalizeArchiveName(name string) string {
	base := name
	ext := filepath.Ext(name)
	if ext != "" {
		base = name[:len(name)-len(ext)]
	}
	if strings.HasSuffix(strings.ToLower(base), ".tar") {
		base = base[:len(base)-4]
	}
	base = strings.ToLower(base)
	for _, sep := range []string{"-", "_", ".", " "} {
		base = strings.ReplaceAll(base, sep, "")
	}
	for _, suffix := range []string{"setup", "install", "installer", "win64", "win32", "amd64", "x64", "x86", "64bit", "32bit"} {
		base = strings.TrimSuffix(base, suffix)
	}
	return base
}
