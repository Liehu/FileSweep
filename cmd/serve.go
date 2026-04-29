package cmd

import (
	"fmt"

	"filesweep/internal/server"

	"github.com/spf13/cobra"
)

var servePort int
var serveHost string

var serveCmd = &cobra.Command{
	Use:   "serve",
	Short: "启动 WebUI 服务器",
	Long:  "启动 HTTP 服务器，提供 WebUI 和 REST API 接口。",
	RunE: func(cmd *cobra.Command, args []string) error {
		cfg := getConfig(cmd)

		if servePort != 8080 {
			cfg.Port = servePort
		}
		if serveHost != "0.0.0.0" {
			cfg.Host = serveHost
		}

		fmt.Printf("FileSweep WebUI 启动中... http://localhost:%d\n", cfg.Port)
		return server.Start(cfg, StaticFiles)
	},
}

func init() {
	rootCmd.AddCommand(serveCmd)

	serveCmd.Flags().IntVar(&servePort, "port", 8080, "WebUI 服务端口")
	serveCmd.Flags().StringVar(&serveHost, "host", "0.0.0.0", "WebUI 服务地址")
}
