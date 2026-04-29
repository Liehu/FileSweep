package server

import (
	"fmt"
	"io/fs"
	"log/slog"
	"net/http"

	"filesweep/internal/config"
	"filesweep/internal/db"

	"github.com/gin-gonic/gin"
	"github.com/gorilla/websocket"
)

var wsUpgrader = websocket.Upgrader{
	CheckOrigin: func(r *http.Request) bool { return true },
}

type Server struct {
	Engine *gin.Engine
	DB     *db.CatalogDB
	Hub    *Hub
	Cfg    *config.Config
	Static fs.FS
}

func New(cfg *config.Config, database *db.CatalogDB, staticFS fs.FS) *Server {
	if cfg == nil {
		slog.Warn("Server initialized with nil config, using default config")
		cfg, _ = config.LoadConfig("")
		if cfg == nil {
			// This should really not happen as LoadConfig("") uses defaults
			slog.Error("Failed to load even default config")
			cfg = &config.Config{} 
		}
	}
	gin.SetMode(gin.DebugMode)
	r := gin.New()
	r.Use(gin.Logger(), gin.Recovery())

	hub := NewHub()
	handlers := &Handlers{DB: database, Hub: hub, Cfg: cfg}

	api := r.Group("/api")
	{
		api.GET("/files", handlers.GetFiles)
		api.GET("/files/stats", handlers.GetFileStats)
		api.GET("/files/suggestions", handlers.GetDupInfo)
		api.POST("/scan", handlers.StartScan)
		api.POST("/clean", handlers.StartClean)
		api.GET("/catalog", handlers.GetCatalog)
		api.PUT("/catalog/:id", handlers.UpdateCatalog)
		api.DELETE("/catalog/:id", handlers.DeleteCatalog)
		api.POST("/enrich", handlers.StartEnrich)
		api.GET("/logs", handlers.GetLogs)
		api.POST("/export", handlers.ExportCSV)
		api.GET("/catalog/export", handlers.ExportCatalog)
		api.GET("/settings", handlers.GetSettings)
		api.PUT("/settings", handlers.UpdateSettings)
		// Categories
		api.GET("/categories", handlers.GetCategories)
		api.POST("/categories", handlers.CreateCategory)
		api.PUT("/categories/:id", handlers.UpdateCategory)
		api.DELETE("/categories/:id", handlers.DeleteCategory)
		// Tags
		api.GET("/tags", handlers.GetTags)
		api.POST("/tags", handlers.CreateTag)
		api.PUT("/tags/:id", handlers.UpdateTag)
		api.DELETE("/tags/:id", handlers.DeleteTag)
		// AI Functional Categories (categories.yaml)
		api.GET("/func-categories", handlers.GetFuncCategories)
		api.PUT("/func-categories", handlers.UpdateFuncCategories)
		// Rules (rules.yaml)
		api.GET("/rules", handlers.GetRules)
		api.PUT("/rules", handlers.UpdateRules)
		api.POST("/reset-db", handlers.ResetDB)
		// Revert
		api.POST("/logs/:id/revert", handlers.RevertOperation)
		api.POST("/logs/batch-revert", handlers.BatchRevert)
	}

	r.GET("/ws", func(c *gin.Context) {
		conn, err := wsUpgrader.Upgrade(c.Writer, c.Request, nil)
		if err != nil {
			slog.Error("WebSocket 升级失败", "error", err)
			return
		}
		hub.Register(conn)

		go func() {
			defer hub.Unregister(conn)
			for {
				_, _, err := conn.ReadMessage()
				if err != nil {
					break
				}
			}
		}()
	})

	if staticFS != nil {
		staticSub, err := fs.Sub(staticFS, "frontend/dist")
		if err == nil {
			r.StaticFS("/assets", http.FS(staticSub))
			r.NoRoute(func(c *gin.Context) {
				c.FileFromFS(c.Request.URL.Path, http.FS(staticSub))
			})
		}
	}

	return &Server{
		Engine: r,
		DB:     database,
		Hub:    hub,
		Cfg:    cfg,
		Static: staticFS,
	}
}

func (s *Server) Run(addr string) error {
	slog.Info("启动 FileSweep 服务器", "addr", addr)
	return s.Engine.Run(addr)
}

func Start(cfg *config.Config, staticFS fs.FS) error {
	database, err := db.Open(cfg.DBPath)
	if err != nil {
		return fmt.Errorf("打开数据库失败: %w", err)
	}
	defer database.Close()

	srv := New(cfg, database, staticFS)
	addr := fmt.Sprintf("%s:%d", cfg.Host, cfg.Port)
	return srv.Run(addr)
}
