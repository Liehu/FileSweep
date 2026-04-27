package cmd

import (
	"fmt"
	"log/slog"
	"path/filepath"

	"filesweep/internal/core"
	"filesweep/internal/db"

	"github.com/google/uuid"
	"github.com/spf13/cobra"
)

var cleanDir string
var cleanConfirm bool

var cleanCmd = &cobra.Command{
	Use:   "clean",
	Short: "清理重复和旧版文件",
	Long:  "扫描目录，检测重复和旧版文件，执行清理操作（移动、删除或重命名）。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)
		isDryRun := dryRun || !cleanConfirm

		database, err := db.Open(cfg.DBPath)
		if err != nil {
			return fmt.Errorf("打开数据库失败: %w", err)
		}
		defer database.Close()

		records, _, err := database.GetFileRecords("", "active", "", 1, 100000)
		if err != nil {
			return fmt.Errorf("查询文件记录失败: %w", err)
		}

		if len(records) == 0 {
			fmt.Println("没有找到文件记录，请先运行 scan 命令")
			return nil
		}

		detector := core.NewDedupDetector(true, 2)
		groups := detector.Detect(records)

		if len(groups) == 0 {
			fmt.Println("没有发现重复文件")
			return nil
		}

		classifier, _ := core.NewClassifier(cfg.RulesPath)

		var actions []core.ExecutorAction
		for _, group := range groups {
			for _, dup := range group.Duplicates {
				action := core.ExecutorAction{
					Operation: core.OpDelete,
					Source:    filepath.Join(cleanDir, dup.LocalPath),
					Reason:    group.Reason,
					File:      dup,
				}

				if classifier != nil {
					result := classifier.Classify(dup)
					if result.TargetDir != "Uncategorized" {
						action.Operation = core.OpMove
						action.Dest = filepath.Join(cleanDir, result.TargetDir, dup.Name)
					}
				}

				actions = append(actions, action)
			}
		}

		sessionID := uuid.New().String()[:8]
		executor := core.NewExecutor(isDryRun, database, cleanDir)

		slog.Info("开始清理", "操作数", len(actions), "dry-run", isDryRun)
		logs, err := executor.Execute(actions, sessionID)
		if err != nil {
			return fmt.Errorf("执行清理失败: %w", err)
		}

		moveCount, deleteCount, failCount := 0, 0, 0
		for _, l := range logs {
			switch {
			case l.Status == "dry_run":
				if l.Operation == "MOVE" {
					moveCount++
				} else {
					deleteCount++
				}
			case l.Status == "success":
				if l.Operation == "MOVE" {
					moveCount++
				} else {
					deleteCount++
				}
			case l.Status == "failed":
				failCount++
			}
		}

		if isDryRun {
			fmt.Printf("[预览模式] 将移动 %d 个文件，删除 %d 个文件\n", moveCount, deleteCount)
		} else {
			fmt.Printf("清理完成: 移动 %d 个，删除 %d 个，失败 %d 个\n", moveCount, deleteCount, failCount)
		}

		if failCount > 0 || len(logs) > 0 {
			csvPath := filepath.Join(filepath.Dir(cfg.DBPath), fmt.Sprintf("clean_%s.csv", sessionID))
			if err := core.ExportCSV(logs, csvPath); err != nil {
				slog.Error("导出 CSV 失败", "error", err)
			} else {
				fmt.Printf("操作日志已保存: %s\n", csvPath)
			}
		}

		return nil
	},
}

func init() {
	rootCmd.AddCommand(cleanCmd)

	cleanCmd.Flags().StringVar(&cleanDir, "dir", "", "清理目录路径")
	cleanCmd.Flags().BoolVar(&cleanConfirm, "confirm", false, "确认执行清理（否则仅预览）")
	cleanCmd.MarkFlagRequired("dir")
}
