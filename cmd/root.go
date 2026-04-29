package cmd

import (
	"context"
	"fmt"
	"io/fs"
	"log/slog"
	"os"

	"filesweep/internal/config"

	"github.com/spf13/cobra"
)

var cfgFile string
var verbose bool
var dryRun bool
var appConfig *config.Config
var StaticFiles fs.FS

type contextKey string

const configKey contextKey = "config"

var rootCmd = &cobra.Command{
	Use:   "filesweep",
	Short: "FileSweep - 文件重复/旧版检测与清理工具",
	Long:  "FileSweep 是一个文件管理工具，用于扫描目录、检测重复和旧版文件、分类整理并清理。",
	PersistentPreRun: func(cmd *cobra.Command, args []string) {
		var err error
		appConfig, err = config.LoadConfig(cfgFile)
		if err != nil {
			fmt.Fprintln(os.Stderr, "加载配置失败:", err)
			os.Exit(1)
		}

		level := slog.LevelInfo
		if verbose {
			level = slog.LevelDebug
		}
		logger := slog.New(slog.NewTextHandler(os.Stderr, &slog.HandlerOptions{Level: level}))
		slog.SetDefault(logger)

		ctx := context.WithValue(cmd.Context(), configKey, appConfig)
		cmd.SetContext(ctx)
	},
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func init() {
	rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "配置文件路径 (默认 ~/.filesweep/config.yaml)")
	rootCmd.PersistentFlags().BoolVarP(&verbose, "verbose", "v", false, "详细输出")
	rootCmd.PersistentFlags().BoolVar(&dryRun, "dry-run", false, "预览模式，不执行实际更改")
}

func getConfig(cmd *cobra.Command) *config.Config {
	if appConfig != nil {
		return appConfig
	}
	if cfg, ok := cmd.Context().Value(configKey).(*config.Config); ok {
		return cfg
	}
	cfg, _ := config.LoadConfig(cfgFile)
	return cfg
}
