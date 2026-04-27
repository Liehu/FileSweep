package tests

import (
	"testing"

	"filesweep/internal/core"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func classifierRules() core.RulesConfig {
	return core.RulesConfig{
		Categories: []core.CategoryRule{
			{
				Name: "安装包", TargetPath: "Installers",
				Extensions:   []string{".exe", ".msi", ".pkg", ".dmg", ".deb", ".rpm", ".AppImage"},
				NameKeywords: []string{"setup", "install", "installer", "update"},
			},
			{
				Name: "脚本", TargetPath: "Scripts",
				SubCategories: []core.CategoryRule{
					{Name: "Shell", TargetPath: "Shell", Extensions: []string{".sh", ".bash", ".zsh"}},
					{Name: "Python", TargetPath: "Python", Extensions: []string{".py"}},
					{Name: "Windows", TargetPath: "Windows", Extensions: []string{".bat", ".cmd", ".ps1"}},
				},
			},
			{
				Name: "Java工具", TargetPath: "Jars",
				Extensions: []string{".jar"},
			},
			{
				Name: "文档", TargetPath: "Docs",
				Extensions: []string{".pdf", ".docx", ".md", ".txt", ".rst", ".epub"},
			},
			{
				Name: "压缩包", TargetPath: "Archives",
				Extensions: []string{".zip", ".7z", ".rar", ".gz", ".tar", ".xz", ".bz2"},
			},
		},
	}
}

func TestClassifier(t *testing.T) {
	c := core.NewClassifierWithRules(classifierRules())

	tests := []struct {
		filename    string
		ext         string
		wantCat     string
		wantTarget  string
	}{
		{"nmap-7.94-setup.exe", ".exe", "安装包", "Installers"},
		{"deploy_prod.sh", ".sh", "脚本/Shell", "Scripts/Shell"},
		{"hutool-all-5.8.22.jar", ".jar", "Java工具", "Jars"},
		{"API设计规范.pdf", ".pdf", "文档", "Docs"},
		{"tools-pack.zip", ".zip", "压缩包", "Archives"},
		{"backup_20240101.py", ".py", "脚本/Python", "Scripts/Python"},
		{"unknown.xyz", ".xyz", "未分类", "Uncategorized"},
		{"run.bat", ".bat", "脚本/Windows", "Scripts/Windows"},
		{"readme.md", ".md", "文档", "Docs"},
	}

	for _, tt := range tests {
		t.Run(tt.filename, func(t *testing.T) {
			rec := core.FileRecord{Name: tt.filename, Extension: tt.ext}
			result := c.Classify(rec)
			assert.Equal(t, tt.wantCat, result.Category)
			assert.Equal(t, tt.wantTarget, result.TargetDir)
		})
	}
}

func TestClassifier_RulesFile(t *testing.T) {
	c, err := core.NewClassifier("../config/rules.yaml")
	require.NoError(t, err)

	rec := core.FileRecord{Name: "nmap-7.94-setup.exe", Extension: ".exe"}
	result := c.Classify(rec)
	assert.Equal(t, "安装包", result.Category)
	assert.Equal(t, "Installers", result.TargetDir)
}

func TestClassifier_RedundantArchive(t *testing.T) {
	c := core.NewClassifierWithRules(classifierRules())

	files := []core.FileRecord{
		{ID: "rec_1", Name: "nmap-7.94-setup.exe", Extension: ".exe"},
		{ID: "rec_2", Name: "nmap-7.94.zip", Extension: ".zip"},
		{ID: "rec_3", Name: "other-tool.zip", Extension: ".zip"},
	}

	assert.True(t, c.IsRedundantArchive(files[1], files), "nmap-7.94.zip should be redundant")
	assert.False(t, c.IsRedundantArchive(files[2], files), "other-tool.zip should not be redundant")
}
