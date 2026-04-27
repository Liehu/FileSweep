package cmd

import (
	"fmt"

	"github.com/spf13/cobra"
)

var importFile string
var importMerge bool

var importCmd = &cobra.Command{
	Use:   "import",
	Short: "导入数据",
	Long:  "从 CSV 文件导入目录数据。",
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Println("import 命令尚未实现")
		fmt.Printf("  文件: %s\n  合并: %v\n", importFile, importMerge)
	},
}

func init() {
	rootCmd.AddCommand(importCmd)

	importCmd.Flags().StringVar(&importFile, "file", "", "导入文件路径")
	importCmd.Flags().BoolVar(&importMerge, "merge", false, "合并模式（不覆盖已有数据）")
	importCmd.MarkFlagRequired("file")
}
