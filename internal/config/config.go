package config

import (
	"fmt"
	"log"
	"os"
	"path/filepath"
	"strings"

	"github.com/spf13/viper"
)

type Config struct {
	ScanDir       string
	Recursive     bool
	AIProvider    string
	AIAPIKey      string
	AIBaseURL     string
	AIConcurrency int
	DBPath        string
	RulesPath     string
	PrivacyRules  []string
	Port          int
	Host          string
	LogLevel      string
	OllamaURL     string
	OpenAIKey     string
	OpenAIBaseURL string
	ClaudeKey     string
	ClaudeBaseURL string
	OllamaModel   string
	// Custom AI provider
	CustomAIName  string
	CustomAIURL   string
	CustomAIKey   string
	CustomAIModel string
	// Settings from frontend
	Rules     RulesSettings
	Privacy   PrivacySettings
	Organize  []OrganizeRule
}

type RulesSettings struct {
	AutoCategorize    bool `json:"autoCategorize"`
	AutoDuplicate     bool `json:"autoDuplicate"`
	KeepNewestVersion bool `json:"keepNewestVersion"`
	DeleteEmptyDirs   bool `json:"deleteEmptyDirs"`
	MoveToRecycleBin  bool `json:"moveToRecycleBin"`
	MinFileSize       int  `json:"minFileSize"`
	MaxFileSize       int  `json:"maxFileSize"`
	IgnorePatterns    string `json:"ignorePatterns"`
}

type PrivacySettings struct {
	ShareHashes      bool `json:"shareHashes"`
	ShareMetadata    bool `json:"shareMetadata"`
	AnalyticsEnabled bool `json:"analyticsEnabled"`
	LogRetentionDays int  `json:"logRetentionDays"`
}

type OrganizeRule struct {
	ID      int    `json:"id"`
	Pattern string `json:"pattern"`
	Target  string `json:"target"`
	Enabled bool   `json:"enabled"`
}

func configBaseDir() string {
	// When running via go run, exe is in a temp go-build directory.
	// Prefer CWD in that case so config/ is found relative to the project.
	if exe, err := os.Executable(); err == nil {
		exeDir := filepath.Dir(exe)
		if strings.Contains(strings.ToLower(exeDir), filepath.Join("temp", "go-build")) {
			if cwd, err := os.Getwd(); err == nil {
				return cwd
			}
		}
	}
	if exe, err := os.Executable(); err == nil {
		return filepath.Dir(exe)
	}
	return "."
}

func defaultConfig() *Config {
	dir := filepath.Join(configBaseDir(), "config")
	return &Config{
		AIProvider:    "offline",
		AIConcurrency: 5,
		DBPath:        filepath.Join(dir, "catalog.db"),
		RulesPath:     filepath.Join(dir, "rules.yaml"),
		Port:          8080,
		Host:          "0.0.0.0",
		LogLevel:      "info",
		OllamaURL:     "http://localhost:11434",
		Rules: RulesSettings{
			AutoCategorize:    true,
			AutoDuplicate:     true,
			KeepNewestVersion: true,
			MoveToRecycleBin:  true,
		},
		Privacy: PrivacySettings{
			LogRetentionDays: 30,
		},
	}
}

func LoadConfig(cfgFile string) (*Config, error) {
	v := viper.New()

	if cfgFile != "" {
		v.SetConfigFile(cfgFile)
	} else {
		// Prefer CWD-based paths (for go run / development)
		v.AddConfigPath("config")
		v.AddConfigPath(".")
		// Fall back to exe-based paths (for installed binary)
		v.AddConfigPath(filepath.Join(configBaseDir(), "config"))
		v.AddConfigPath(configBaseDir())
		v.SetConfigName("config")
	}

	v.SetEnvPrefix("FILESWEEP")
	v.AutomaticEnv()

	if err := v.ReadInConfig(); err != nil {
		if _, ok := err.(viper.ConfigFileNotFoundError); !ok {
			return nil, fmt.Errorf("读取配置文件失败: %w", err)
		}
	}

	def := defaultConfig()

	cfg := &Config{
		ScanDir:       v.GetString("scanDir"),
		Recursive:     v.GetBool("recursive"),
		AIProvider:    v.GetString("aiProvider"),
		AIAPIKey:      v.GetString("aiApiKey"),
		AIBaseURL:     v.GetString("aiBaseUrl"),
		AIConcurrency: v.GetInt("aiConcurrency"),
		DBPath:        v.GetString("dbPath"),
		RulesPath:     v.GetString("rulesPath"),
		PrivacyRules:  v.GetStringSlice("privacyRules"),
		Port:          v.GetInt("port"),
		Host:          v.GetString("host"),
		LogLevel:      v.GetString("logLevel"),
		OllamaURL:     v.GetString("ollamaUrl"),
		OllamaModel:   v.GetString("ollamaModel"),
		OpenAIKey:     v.GetString("openaiKey"),
		OpenAIBaseURL: v.GetString("openaiBaseUrl"),
		ClaudeKey:     v.GetString("claudeKey"),
		ClaudeBaseURL: v.GetString("claudeBaseUrl"),
		CustomAIName:  v.GetString("customAiName"),
		CustomAIURL:   v.GetString("customAiUrl"),
		CustomAIKey:   v.GetString("customAiKey"),
		CustomAIModel: v.GetString("customAiModel"),
	}

	v.UnmarshalKey("rules", &cfg.Rules)
	v.UnmarshalKey("privacy", &cfg.Privacy)

	// Apply defaults
	if cfg.AIProvider == "" {
		cfg.AIProvider = def.AIProvider
	}
	if cfg.AIConcurrency == 0 {
		cfg.AIConcurrency = def.AIConcurrency
	}
	if cfg.DBPath == "" {
		cfg.DBPath = def.DBPath
	}
	if cfg.RulesPath == "" {
		cfg.RulesPath = def.RulesPath
	}
	if cfg.Port == 0 {
		cfg.Port = def.Port
	}
	if cfg.Host == "" {
		cfg.Host = def.Host
	}
	if cfg.LogLevel == "" {
		cfg.LogLevel = def.LogLevel
	}
	if cfg.OllamaURL == "" {
		cfg.OllamaURL = def.OllamaURL
	}

	return cfg, nil
}

func SaveConfig(cfg *Config, path string) error {
	v := viper.New()

	v.Set("scanDir", cfg.ScanDir)
	v.Set("recursive", cfg.Recursive)
	v.Set("aiProvider", cfg.AIProvider)
	v.Set("aiApiKey", cfg.AIAPIKey)
	v.Set("aiBaseUrl", cfg.AIBaseURL)
	v.Set("aiConcurrency", cfg.AIConcurrency)
	v.Set("dbPath", cfg.DBPath)
	v.Set("rulesPath", cfg.RulesPath)
	v.Set("privacyRules", cfg.PrivacyRules)
	v.Set("port", cfg.Port)
	v.Set("host", cfg.Host)
	v.Set("logLevel", cfg.LogLevel)
	v.Set("ollamaUrl", cfg.OllamaURL)
	v.Set("ollamaModel", cfg.OllamaModel)
	v.Set("openaiKey", cfg.OpenAIKey)
	v.Set("openaiBaseUrl", cfg.OpenAIBaseURL)
	v.Set("claudeKey", cfg.ClaudeKey)
	v.Set("claudeBaseUrl", cfg.ClaudeBaseURL)
	v.Set("customAiName", cfg.CustomAIName)
	v.Set("customAiUrl", cfg.CustomAIURL)
	v.Set("customAiKey", cfg.CustomAIKey)
	v.Set("customAiModel", cfg.CustomAIModel)

	v.Set("rules", map[string]any{
		"autoCategorize":    cfg.Rules.AutoCategorize,
		"autoDuplicate":     cfg.Rules.AutoDuplicate,
		"keepNewestVersion": cfg.Rules.KeepNewestVersion,
		"deleteEmptyDirs":   cfg.Rules.DeleteEmptyDirs,
		"moveToRecycleBin":  cfg.Rules.MoveToRecycleBin,
		"minFileSize":       cfg.Rules.MinFileSize,
		"maxFileSize":       cfg.Rules.MaxFileSize,
		"ignorePatterns":    cfg.Rules.IgnorePatterns,
	})
	v.Set("privacy", map[string]any{
		"shareHashes":      cfg.Privacy.ShareHashes,
		"shareMetadata":    cfg.Privacy.ShareMetadata,
		"analyticsEnabled": cfg.Privacy.AnalyticsEnabled,
		"logRetentionDays": cfg.Privacy.LogRetentionDays,
	})

	v.SetConfigFile(path)
	v.SetConfigType("yaml")

	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return fmt.Errorf("创建配置目录失败: %w", err)
	}

	log.Printf("[SaveConfig] 尝试写入配置文件: %s", path)
	if err := v.WriteConfig(); err != nil {
		log.Printf("[SaveConfig] WriteConfig 失败: %v", err)
		return fmt.Errorf("写入配置文件失败: %w", err)
	}

	log.Printf("[SaveConfig] 配置文件写入成功")
	return nil
}

func DefaultConfigPath() string {
	return filepath.Join(configBaseDir(), "config", "config.yaml")
}
