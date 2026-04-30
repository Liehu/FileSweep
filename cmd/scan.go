package cmd

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"

	"filesweep/internal/config"
	"filesweep/internal/core"
	"filesweep/internal/db"

	"github.com/spf13/cobra"
)

var scanDir string
var scanRecursive bool
var scanOutput string

var scanCmd = &cobra.Command{
	Use:   "scan",
	Short: "扫描目录中的文件",
	Long:  "扫描指定目录，收集文件信息并计算 SHA-256 哈希值。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)

		scanner := core.NewScanner()

		go func() {
			for p := range scanner.ProgressCh {
				if verbose {
					slog.Info("扫描进度", "stage", p.Stage, "total", p.Total, "done", p.Done, "current", p.CurrentFile)
				}
			}
		}()

		slog.Info("开始扫描", "dir", scanDir, "recursive", scanRecursive)
		records, err := scanner.Scan(context.Background(), scanDir, scanRecursive, false)
		if err != nil {
			return fmt.Errorf("扫描失败: %w", err)
		}
		slog.Info("扫描完成", "文件数", len(records))

		rulesPath := ensureRulesFile(cfg)
		classifier, err := core.NewClassifier(rulesPath)
		if err != nil {
			slog.Warn("加载分类规则失败，跳过分类", "error", err)
		}

		for i := range records {
			if v, ok := core.ExtractVersion(records[i].Name); ok {
				records[i].Version = v
			}
			if classifier != nil {
				result := classifier.Classify(records[i])
				records[i].Category = result.Category
			}
		}

		dbPath := cfg.DBPath
		if scanOutput != "" {
			dbPath = scanOutput
		}

		database, err := db.Open(dbPath)
		if err != nil {
			return fmt.Errorf("打开数据库失败: %w", err)
		}
		defer database.Close()

		if err := database.BatchInsertFileRecords(records); err != nil {
			return fmt.Errorf("保存扫描结果失败: %w", err)
		}

		fmt.Printf("扫描完成: %d 个文件已保存到 %s\n", len(records), dbPath)

		detector := core.NewDedupDetector(true, 2)
		groups := detector.Detect(records)
		if len(groups) > 0 {
			totalDupes := 0
			for _, g := range groups {
				totalDupes += len(g.Duplicates)
			}
			fmt.Printf("发现 %d 组重复（共 %d 个重复文件），使用 'filesweep clean' 进行清理\n", len(groups), totalDupes)
		}

		return nil
	},
}

func ensureRulesFile(cfg *config.Config) string {
	if _, err := os.Stat(cfg.RulesPath); err == nil {
		return cfg.RulesPath
	}

	exePath, err := os.Executable()
	if err == nil {
		bundledRules := filepath.Join(filepath.Dir(exePath), "config", "rules.yaml")
		if _, err := os.Stat(bundledRules); err == nil {
			return bundledRules
		}
	}

	localRules := filepath.Join("config", "rules.yaml")
	if _, err := os.Stat(localRules); err == nil {
		abs, _ := filepath.Abs(localRules)
		return abs
	}

	return cfg.RulesPath
}

func init() {
	rootCmd.AddCommand(scanCmd)

	scanCmd.Flags().StringVar(&scanDir, "dir", "", "扫描目录路径")
	scanCmd.Flags().BoolVar(&scanRecursive, "recursive", false, "递归扫描子目录")
	scanCmd.Flags().StringVar(&scanOutput, "output", "", "输出路径 (数据库文件)")
	scanCmd.MarkFlagRequired("dir")
}
