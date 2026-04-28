package core

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
)

type CategoryRule struct {
	Name          string         `yaml:"name"`
	TargetPath    string         `yaml:"target_path"`
	Extensions    []string       `yaml:"extensions"`
	NameKeywords  []string       `yaml:"name_keywords"`
	SubCategories []CategoryRule `yaml:"sub_categories"`
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

func (c *Classifier) Classify(file FileRecord) ClassifyResult {
	for _, rule := range c.Rules.Categories {
		if matched, subName, subTarget := c.matchRule(rule, file); matched {
			category := rule.Name
			targetDir := rule.TargetPath
			if subName != "" {
				category = rule.Name + "/" + subName
				targetDir = rule.TargetPath + "/" + subTarget
			}
			return ClassifyResult{Category: category, TargetDir: targetDir}
		}
	}
	return ClassifyResult{Category: "未分类", TargetDir: "Uncategorized"}
}

func (c *Classifier) matchRule(rule CategoryRule, file FileRecord) (matched bool, subName string, subTarget string) {
	extMatch := matchExtension(rule.Extensions, file.Extension)
	kwMatch := matchKeywords(rule.NameKeywords, file.Name)

	// 如果有子分类，先尝试匹配子分类（父级可有自己的匹配条件也可以没有）
	if len(rule.SubCategories) > 0 {
		for _, sub := range rule.SubCategories {
			if matchExtension(sub.Extensions, file.Extension) || matchKeywords(sub.NameKeywords, file.Name) {
				return true, sub.Name, sub.TargetPath
			}
		}
		// 子分类都未命中，如果父级有匹配条件则用父级兜底
		if extMatch || kwMatch {
			return true, "", ""
		}
		return false, "", ""
	}

	if extMatch || kwMatch {
		return true, "", ""
	}
	return false, "", ""
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
	// #11: 处理 .tar.gz / .tar.bz2 双后缀
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
		// #12: 用 TrimSuffix 替代手动切片
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
