package cmd

import (
	"fmt"
	"os"
	"path/filepath"

	"filesweep/internal/db"

	"github.com/spf13/cobra"
)

var exportFormat string
var exportOutput string

var exportCmd = &cobra.Command{
	Use:   "export",
	Short: "导出数据",
	Long:  "将目录数据导出为 CSV 或其他格式。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)

		database, err := db.Open(cfg.DBPath)
		if err != nil {
			return fmt.Errorf("打开数据库失败: %w", err)
		}
		defer database.Close()

		records, _, err := database.GetFileRecords("", "", "", 1, 1000000)
		if err != nil {
			return fmt.Errorf("查询文件记录失败: %w", err)
		}

		output := exportOutput
		if output == "" {
			output = filepath.Join(filepath.Dir(cfg.DBPath), "catalog.csv")
		}

		entries, _, _ := database.GetCatalogEntries("", 1, 1000000)
		entriesMap := make(map[string]string)
		for _, e := range entries {
			entriesMap[e.ID] = e.Name
		}

		if exportFormat != "csv" {
			return fmt.Errorf("不支持的导出格式: %s，目前仅支持 csv", exportFormat)
		}

		f, err := os.Create(output)
		if err != nil {
			return fmt.Errorf("创建输出文件失败: %w", err)
		}
		defer f.Close()

		fmt.Fprintf(f, "id,name,version,category,local_path,file_size,file_hash,ai_skip,description,homepage_url,download_url,latest_version,license,tags,ai_confidence,meta_updated_at,notes\n")
		for _, r := range records {
			fmt.Fprintf(f, "%s,%s,%s,%s,%s,%d,%s,%t,,,,,,,,,\n",
				r.ID, r.Name, r.Version, r.Category, r.LocalPath,
				r.FileSize, r.FileHash, r.AISkip)
		}

		fmt.Printf("已导出 %d 条记录到 %s\n", len(records), output)
		return nil
	},
}

func init() {
	rootCmd.AddCommand(exportCmd)

	exportCmd.Flags().StringVar(&exportFormat, "format", "csv", "导出格式 (csv)")
	exportCmd.Flags().StringVar(&exportOutput, "output", "", "输出文件路径")
}
