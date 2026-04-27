package cmd

import (
	"fmt"
	"log/slog"
	"strings"

	"filesweep/internal/config"

	"github.com/spf13/cobra"
)

var configSet string

var configCmd = &cobra.Command{
	Use:   "config",
	Short: "管理配置",
	Long:  "查看或修改 FileSweep 配置。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)

		if configSet == "" {
			fmt.Println("当前配置:")
			fmt.Printf("  AI Provider:    %s\n", cfg.AIProvider)
			fmt.Printf("  AI Base URL:    %s\n", cfg.AIBaseURL)
			fmt.Printf("  AI Concurrency: %d\n", cfg.AIConcurrency)
			fmt.Printf("  DB Path:        %s\n", cfg.DBPath)
			fmt.Printf("  Rules Path:     %s\n", cfg.RulesPath)
			fmt.Printf("  Port:           %d\n", cfg.Port)
			fmt.Printf("  Host:           %s\n", cfg.Host)
			fmt.Printf("  Log Level:      %s\n", cfg.LogLevel)
			return nil
		}

		parts := strings.SplitN(configSet, "=", 2)
		if len(parts) != 2 {
			return fmt.Errorf("格式错误，请使用 key=value，例如: ai.provider=claude")
		}
		key, value := parts[0], parts[1]

		switch key {
		case "ai.provider", "aiProvider":
			cfg.AIProvider = value
		case "ai.apiKey", "aiApiKey":
			cfg.AIAPIKey = value
		case "ai.baseUrl", "aiBaseUrl":
			cfg.AIBaseURL = value
		case "ai.concurrency", "aiConcurrency":
			fmt.Sscanf(value, "%d", &cfg.AIConcurrency)
		case "dbPath":
			cfg.DBPath = value
		case "rulesPath":
			cfg.RulesPath = value
		case "port":
			fmt.Sscanf(value, "%d", &cfg.Port)
		case "host":
			cfg.Host = value
		case "logLevel":
			cfg.LogLevel = value
		default:
			return fmt.Errorf("未知的配置项: %s", key)
		}

		configPath := cfg.DBPath
		if configPath != "" {
			configPath = configPath[:len(configPath)-len("catalog.db")] + "config.yaml"
		}
		if err := config.SaveConfig(cfg, configPath); err != nil {
			return fmt.Errorf("保存配置失败: %w", err)
		}

		slog.Info("配置已更新", "key", key, "value", value)
		fmt.Printf("已设置 %s = %s\n", key, value)
		return nil
	},
}

func init() {
	rootCmd.AddCommand(configCmd)

	configCmd.Flags().StringVar(&configSet, "set", "", "设置配置项 (格式: key=value)")
}
