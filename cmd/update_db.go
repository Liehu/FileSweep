package cmd

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"filesweep/internal/ai"

	"github.com/spf13/cobra"
)

var updateDBFromFile string

var updateDBCmd = &cobra.Command{
	Use:   "update-db",
	Short: "更新离线知识库",
	Long:  "更新预置的软件元数据离线知识库。支持 --from-file 从 JSON 文件加载自定义条目。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)

		dbPath := filepath.Join(filepath.Dir(cfg.DBPath), "offline_db.sqlite")

		entries := ai.DefaultOfflineEntries()

		if updateDBFromFile != "" {
			custom, err := loadEntriesFromFile(updateDBFromFile)
			if err != nil {
				return fmt.Errorf("加载自定义条目失败: %w", err)
			}
			entries = append(entries, custom...)
			fmt.Printf("从 %s 加载了 %d 条自定义条目\n", updateDBFromFile, len(custom))
		}

		if err := ai.CreateOfflineDB(dbPath, entries); err != nil {
			return fmt.Errorf("创建离线知识库失败: %w", err)
		}

		fmt.Printf("离线知识库已更新: %d 条记录 -> %s\n", len(entries), dbPath)
		return nil
	},
}

func loadEntriesFromFile(path string) ([]ai.OfflineEntry, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	var entries []ai.OfflineEntry
	if err := json.Unmarshal(data, &entries); err != nil {
		return nil, fmt.Errorf("解析 JSON 失败: %w", err)
	}
	return entries, nil
}

func init() {
	rootCmd.AddCommand(updateDBCmd)
	updateDBCmd.Flags().StringVar(&updateDBFromFile, "from-file", "", "从 JSON 文件加载自定义知识库条目")
}
