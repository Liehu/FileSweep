package cmd

import (
	"context"
	"fmt"
	"log/slog"
	"path/filepath"
	"time"

	"filesweep/internal/ai"
	"filesweep/internal/core"
	"filesweep/internal/db"

	"github.com/spf13/cobra"
)

var enrichAIProvider string
var enrichSkipPrivate bool
var enrichConcurrency int

var enrichCmd = &cobra.Command{
	Use:   "enrich",
	Short: "AI 丰富文件元数据",
	Long:  "使用 AI 为文件添加描述、主页、最新版本等元数据信息。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)
		provider := enrichAIProvider
		if provider == "" {
			provider = cfg.AIProvider
		}

		database, err := db.Open(cfg.DBPath)
		if err != nil {
			return fmt.Errorf("打开数据库失败: %w", err)
		}
		defer database.Close()

		records, _, err := database.GetFileRecords("", "active", "", 1, 1000000)
		if err != nil {
			return fmt.Errorf("查询文件记录失败: %w", err)
		}

		if len(records) == 0 {
			fmt.Println("没有找到文件记录，请先运行 scan 命令")
			return nil
		}

		privacy := core.NewPrivacyChecker(cfg.PrivacyRules)
		var requests []ai.EnrichRequest
		var validRecords []core.FileRecord

		for _, r := range records {
			if enrichSkipPrivate && (r.AISkip || privacy.ShouldSkip(r)) {
				slog.Info("跳过私密文件", "name", r.Name)
				continue
			}
			requests = append(requests, ai.EnrichRequest{
				Name:      r.Name,
				Version:   r.Version,
				Extension: r.Extension,
				Category:  r.Category,
				FileSize:  r.FileSize,
			})
			validRecords = append(validRecords, r)
		}

		if len(requests) == 0 {
			fmt.Println("没有需要补全的文件")
			return nil
		}

		fmt.Printf("开始 AI 补全: %d 个文件, 提供方: %s, 并发: %d\n", len(requests), provider, enrichConcurrency)

		offlineDB := filepath.Join(filepath.Dir(cfg.DBPath), "offline_db.sqlite")
		offlineEnricher, err := ai.NewOfflineEnricher(offlineDB)
		if err != nil {
			slog.Warn("离线知识库加载失败", "error", err)
		}
		if offlineEnricher != nil {
			defer offlineEnricher.Close()
		}

		var enricher ai.Enricher
		switch provider {
		case "openai":
			enricher = ai.NewOpenAIEnricher(cfg.AIAPIKey, cfg.AIBaseURL)
		case "claude":
			enricher = ai.NewClaudeEnricher(cfg.AIAPIKey, cfg.AIBaseURL)
		case "ollama":
			enricher = ai.NewOllamaEnricher(cfg.AIBaseURL)
		case "offline":
			if offlineEnricher != nil {
				enricher = offlineEnricher
			} else {
				return fmt.Errorf("离线知识库不可用")
			}
		default:
			return fmt.Errorf("不支持的 AI 提供方: %s", provider)
		}

		progressCh := make(chan ai.EnrichProgress, 16)
		go func() {
			for p := range progressCh {
				if verbose {
					slog.Info("补全进度", "done", p.Done, "total", p.Total, "current", p.Current)
				}
			}
		}()

		results, err := ai.BatchEnrich(context.Background(), enricher, requests, enrichConcurrency, progressCh)
		if err != nil {
			return fmt.Errorf("AI 补全失败: %w", err)
		}

		reviewCount := 0
		for i, result := range results {
			if i >= len(validRecords) {
				break
			}
			entry := core.CatalogEntry{
				ID:            fmt.Sprintf("cat_%s", validRecords[i].FileHash[:8]),
				Name:          validRecords[i].Name,
				Description:   result.Description,
				HomepageURL:   result.HomepageURL,
				DownloadURL:   result.DownloadURL,
				LatestVersion: result.LatestVersion,
				License:       result.License,
				Tags:          result.Tags,
				AIConfidence:  result.Confidence,
				AIProvider:    result.Provider,
				MetaUpdatedAt: time.Now().UTC(),
				NeedsReview:   result.NeedsReview,
			}

			if result.NeedsReview {
				reviewCount++
			}

			if err := database.InsertCatalogEntry(entry); err != nil {
				slog.Error("保存目录条目失败", "name", entry.Name, "error", err)
			}
		}

		fmt.Printf("补全完成: %d/%d 个文件已处理, %d 个需人工审核\n", len(results), len(requests), reviewCount)
		return nil
	},
}

func init() {
	rootCmd.AddCommand(enrichCmd)

	enrichCmd.Flags().StringVar(&enrichAIProvider, "ai-provider", "", "AI 提供者 (openai/claude/ollama/offline)")
	enrichCmd.Flags().BoolVar(&enrichSkipPrivate, "skip-private", false, "跳过私有/敏感文件")
	enrichCmd.Flags().IntVar(&enrichConcurrency, "concurrency", 5, "并发请求数")
}
