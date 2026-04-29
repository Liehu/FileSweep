package server

import (
	"bytes"
	"context"
	"encoding/csv"
	"fmt"
	"log/slog"
	"net/http"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"filesweep/internal/ai"
	"filesweep/internal/config"
	"filesweep/internal/core"
	"filesweep/internal/db"

	"github.com/gin-gonic/gin"
	"github.com/google/uuid"
	"gopkg.in/yaml.v3"
)

type Handlers struct {
	DB  *db.CatalogDB
	Hub *Hub
	Cfg *config.Config
}

func (h *Handlers) GetFiles(c *gin.Context) {
	category := c.Query("category")
	status := c.Query("status")
	search := c.Query("search")
	if q := c.Query("q"); q != "" {
		search = q
	}
	page, _ := strconv.Atoi(c.DefaultQuery("page", "1"))
	pageSize, _ := strconv.Atoi(c.DefaultQuery("page_size", "50"))
	if pageSize > 200 {
		pageSize = 200
	}
	records, total, err := h.DB.GetFileRecords(category, status, search, page, pageSize)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"data":      records,
		"total":     total,
		"page":      page,
		"page_size": pageSize,
	})
}

func (h *Handlers) GetFileStats(c *gin.Context) {
	stats, err := h.DB.GetFileStats()
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"total":         stats.Total,
		"duplicates":    stats.Duplicates,
		"multiversion":  stats.Multiversion,
		"uncategorized": stats.Uncategorized,
		"total_size":    stats.TotalSize,
	})
}

func (h *Handlers) GetSettings(c *gin.Context) {
	cfg := h.Cfg
	if cfg == nil {
		slog.Error("GetSettings: Config is nil")
		c.JSON(http.StatusInternalServerError, gin.H{"error": "配置未加载"})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"rules": gin.H{
			"autoCategorize":    cfg.Rules.AutoCategorize,
			"autoDuplicate":     cfg.Rules.AutoDuplicate,
			"keepNewestVersion": cfg.Rules.KeepNewestVersion,
			"deleteEmptyDirs":   cfg.Rules.DeleteEmptyDirs,
			"moveToRecycleBin":  cfg.Rules.MoveToRecycleBin,
			"minFileSize":       cfg.Rules.MinFileSize,
			"maxFileSize":       cfg.Rules.MaxFileSize,
			"ignorePatterns":    cfg.Rules.IgnorePatterns,
		},
		"privacy": gin.H{
			"shareHashes":      cfg.Privacy.ShareHashes,
			"shareMetadata":    cfg.Privacy.ShareMetadata,
			"analyticsEnabled": cfg.Privacy.AnalyticsEnabled,
			"logRetentionDays": cfg.Privacy.LogRetentionDays,
		},
		"ai": gin.H{
			"provider":      cfg.AIProvider,
			"ollamaUrl":     cfg.OllamaURL,
			"model":         cfg.OllamaModel,
			"openaiKey":     cfg.OpenAIKey,
			"openaiBaseUrl": cfg.OpenAIBaseURL,
			"claudeKey":     cfg.ClaudeKey,
			"claudeBaseUrl": cfg.ClaudeBaseURL,
			"customName":    cfg.CustomAIName,
			"customUrl":     cfg.CustomAIURL,
			"customKey":     cfg.CustomAIKey,
			"customModel":   cfg.CustomAIModel,
		},
	})
}

func (h *Handlers) UpdateSettings(c *gin.Context) {
	cfg := h.Cfg
	if cfg == nil {
		slog.Error("UpdateSettings: Config is nil")
		c.JSON(http.StatusInternalServerError, gin.H{"error": "配置未加载"})
		return
	}

	var req struct {
		Rules    *config.RulesSettings   `json:"rules"`
		Privacy  *config.PrivacySettings `json:"privacy"`
		AI       map[string]interface{}  `json:"ai"`
	}
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}

	if req.Rules != nil {
		cfg.Rules = *req.Rules
	}
	if req.Privacy != nil {
		cfg.Privacy = *req.Privacy
	}
	if req.AI != nil {
		if v, ok := req.AI["provider"].(string); ok {
			cfg.AIProvider = v
		}
		if v, ok := req.AI["ollamaUrl"].(string); ok {
			cfg.OllamaURL = v
		}
		if v, ok := req.AI["model"].(string); ok {
			cfg.OllamaModel = v
		}
		if v, ok := req.AI["openaiKey"].(string); ok {
			cfg.OpenAIKey = v
		}
		if v, ok := req.AI["openaiBaseUrl"].(string); ok {
			cfg.OpenAIBaseURL = v
		}
		if v, ok := req.AI["claudeKey"].(string); ok {
			cfg.ClaudeKey = v
		}
		if v, ok := req.AI["claudeBaseUrl"].(string); ok {
			cfg.ClaudeBaseURL = v
		}
		if v, ok := req.AI["customName"].(string); ok {
			cfg.CustomAIName = v
		}
		if v, ok := req.AI["customUrl"].(string); ok {
			cfg.CustomAIURL = v
		}
		if v, ok := req.AI["customKey"].(string); ok {
			cfg.CustomAIKey = v
		}
		if v, ok := req.AI["customModel"].(string); ok {
			cfg.CustomAIModel = v
		}
	}

	if err := config.SaveConfig(cfg, config.DefaultConfigPath()); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "保存配置失败: " + err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "saved"})
}

type ScanRequest struct {
	Dirs         []string `json:"dirs"`
	Recursive    bool     `json:"recursive"`
	ExcludeDirs  []string `json:"exclude_dirs"`
	ExcludeNames []string `json:"exclude_names"`
	ExcludeExts  []string `json:"exclude_exts"`
}

func (h *Handlers) StartScan(c *gin.Context) {
	var req ScanRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}
	dirs := req.Dirs
	if len(dirs) == 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "dirs 参数必填"})
		return
	}
	go func() {
		defer func() {
			if r := recover(); r != nil {
				slog.Error("scan goroutine panic", "error", r)
				h.Hub.Broadcast("scan_error", gin.H{"error": fmt.Sprintf("%v", r)})
			}
		}()
		var allRecords []core.FileRecord
		scanner := core.NewScanner()
		defer close(scanner.ProgressCh)
		go func() {
			for p := range scanner.ProgressCh {
				percent := 0
				if p.Total > 0 {
					percent = p.Done * 100 / p.Total
				}
				h.Hub.Broadcast("scan_progress", gin.H{
					"total":       p.Total,
					"done":        p.Done,
					"currentFile": p.CurrentFile,
					"stage":       p.Stage,
					"percent":     percent,
				})
			}
		}()
		for _, dir := range dirs {
			records, err := scanner.Scan(context.Background(), dir, req.Recursive)
			if err != nil {
				h.Hub.Broadcast("scan_error", gin.H{"error": err.Error(), "dir": dir})
				continue
			}
			allRecords = append(allRecords, records...)
		}
		// Apply exclusions
		if len(req.ExcludeExts) > 0 || len(req.ExcludeNames) > 0 || len(req.ExcludeDirs) > 0 {
			extSet := map[string]bool{}
			for _, e := range req.ExcludeExts {
				extSet[strings.ToLower(e)] = true
			}
			nameSet := map[string]bool{}
			for _, n := range req.ExcludeNames {
				nameSet[strings.ToLower(n)] = true
			}
			filtered := allRecords[:0]
			for _, r := range allRecords {
				if extSet[strings.ToLower(r.Extension)] {
					continue
				}
				if nameSet[strings.ToLower(r.Name)] {
					continue
				}
				skip := false
				for _, excDir := range req.ExcludeDirs {
					if strings.Contains(strings.ToLower(r.LocalPath), strings.ToLower(excDir)) {
						skip = true
						break
					}
				}
				if skip {
					continue
				}
				filtered = append(filtered, r)
			}
			allRecords = filtered
		}
		classifier := core.NewClassifierWithDefaults()
		if rulesPath := ensureRulesPath(h.Cfg); rulesPath != "" {
			if c, err := core.NewClassifier(rulesPath); err == nil && len(c.Rules.Categories) > 0 {
				classifier = c
			}
		}
		for i := range allRecords {
			if v, ok := core.ExtractVersion(allRecords[i].Name); ok {
				allRecords[i].Version = v
			}
			if classifier != nil {
				result := classifier.Classify(allRecords[i])
				allRecords[i].Category = result.Category
			}
		}
		if err := h.DB.BatchInsertFileRecords(allRecords); err != nil {
			h.Hub.Broadcast("scan_error", gin.H{"error": err.Error()})
			return
		}
		h.Hub.Broadcast("scan_complete", gin.H{
			"total": len(allRecords),
			"dirs":  dirs,
		})
	}()
	c.JSON(http.StatusAccepted, gin.H{"status": "scanning", "dirs": dirs})
}

type FileActionEntry struct {
	ID     string `json:"id"`
	Action string `json:"action"`
	Target string `json:"target"`
}

type CleanRequest struct {
	Confirm     bool              `json:"confirm"`
	SelectedIDs []string          `json:"selected_ids"`
	FileActions []FileActionEntry `json:"file_actions"`
}

func (h *Handlers) StartClean(c *gin.Context) {
	var req CleanRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}

	all, _, err := h.DB.GetFileRecords("", "", "", 1, 1000000)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	// Build lookup
	recordMap := map[string]core.FileRecord{}
	for _, r := range all {
		recordMap[r.ID] = r
	}

	// Dedup info for reasons
	detector := core.NewDedupDetector(true, 2)
	groups := detector.Detect(all)
	dupReasons := map[string]string{}
	dupIDs := map[string]bool{}
	for _, g := range groups {
		for _, d := range g.Duplicates {
			dupReasons[d.ID] = g.Reason
			dupIDs[d.ID] = true
		}
	}

	classifier := core.NewClassifierWithDefaults()
	if rulesPath := ensureRulesPath(h.Cfg); rulesPath != "" {
		if c, err := core.NewClassifier(rulesPath); err == nil && len(c.Rules.Categories) > 0 {
			classifier = c
		}
	}
	var actions []core.ExecutorAction

	if len(req.FileActions) > 0 {
		// User specified per-file actions from frontend
		for _, fa := range req.FileActions {
			r, ok := recordMap[fa.ID]
			if !ok {
				continue
			}
			reason := "用户操作"
			if dupReasons[r.ID] != "" {
				reason = dupReasons[r.ID]
			}
			switch fa.Action {
			case "delete":
				actions = append(actions, core.ExecutorAction{
					Operation: core.OpDelete,
					Source:    r.LocalPath,
					Reason:    reason,
					File:      r,
				})
			case "move", "archive":
				var dest string
				scanBase := filepath.Dir(r.LocalPath)
				if fa.Target != "" {
					dest = filepath.Join(scanBase, fa.Target, r.Name)
				} else if classifier != nil {
					result := classifier.Classify(r)
					if result.TargetDir != "" && result.TargetDir != "Uncategorized" {
						dest = filepath.Join(scanBase, result.TargetDir, r.Name)
					}
				}
				if dest == "" || dest == r.LocalPath {
					continue
				}
				actions = append(actions, core.ExecutorAction{
					Operation: core.OpMove,
					Source:    r.LocalPath,
					Dest:      dest,
					Reason:    reason,
					File:      r,
				})
			}
			// "keep" — skip
		}
	} else if len(req.SelectedIDs) > 0 {
		// Legacy: delete selected
		for _, id := range req.SelectedIDs {
			r, ok := recordMap[id]
			if !ok {
				continue
			}
			reason := "用户选择删除"
			if dupReasons[r.ID] != "" {
				reason = dupReasons[r.ID]
			}
			actions = append(actions, core.ExecutorAction{
				Operation: core.OpDelete,
				Source:    r.LocalPath,
				Reason:    reason,
				File:      r,
			})
		}
	} else {
		// Auto: detect duplicates
		for _, group := range groups {
			for _, dup := range group.Duplicates {
				action := core.ExecutorAction{
					Operation: core.OpDelete,
					Source:    dup.LocalPath,
					Reason:    group.Reason,
					File:      dup,
				}
				if classifier != nil {
					result := classifier.Classify(dup)
					if result.TargetDir != "" && result.TargetDir != "Uncategorized" {
						action.Operation = core.OpMove
						action.Dest = filepath.Join(filepath.Dir(dup.LocalPath), result.TargetDir, dup.Name)
					}
				}
				actions = append(actions, action)
			}
		}
	}

	if len(actions) == 0 {
		c.JSON(http.StatusOK, gin.H{"message": "没有需要处理的操作"})
		return
	}

	sessionID := uuid.New().String()[:8]
	isDryRun := !req.Confirm
	executor := core.NewExecutor(isDryRun, h.DB, "")

	go func() {
		logs, execErr := executor.Execute(actions, sessionID)
		if execErr != nil {
			h.Hub.Broadcast("clean_error", gin.H{"error": execErr.Error()})
			return
		}
		// Remove successfully processed records from DB
		for _, l := range logs {
			if l.Status == "success" {
				_ = h.DB.DeleteFileRecord(l.RecordID)
			}
		}
		h.Hub.Broadcast("clean_complete", gin.H{
			"session_id": sessionID,
			"total":      len(logs),
			"dry_run":    isDryRun,
		})
	}()

	c.JSON(http.StatusAccepted, gin.H{
		"status":     "cleaning",
		"session_id": sessionID,
		"dry_run":    isDryRun,
		"actions":    len(actions),
	})
}

// DuplicateInfo returns dedup info for the file list
func (h *Handlers) GetDupInfo(c *gin.Context) {
	all, _, err := h.DB.GetFileRecords("", "", "", 1, 1000000)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}

	detector := core.NewDedupDetector(true, 2)
	groups := detector.Detect(all)

	classifier := core.NewClassifierWithDefaults()
	if rulesPath := ensureRulesPath(h.Cfg); rulesPath != "" {
		if c, err := core.NewClassifier(rulesPath); err == nil && len(c.Rules.Categories) > 0 {
			classifier = c
		}
	}

	type Suggestion struct {
		ID       string `json:"id"`
		IsDup    bool   `json:"is_dup"`
		Reason   string `json:"reason"`
		Target   string `json:"target"`
		Suggest  string `json:"suggest"`
	}

	suggestions := map[string]Suggestion{}

	for _, r := range all {
		sug := Suggestion{ID: r.ID, IsDup: false}
		if classifier != nil {
			result := classifier.Classify(r)
			if result.TargetDir != "" && result.TargetDir != "Uncategorized" {
				sug.Target = result.TargetDir
				sug.Suggest = "→ " + result.TargetDir + "\\" + r.Name
			}
		}
		suggestions[r.ID] = sug
	}

	for _, g := range groups {
		for _, d := range g.Duplicates {
			if sug, ok := suggestions[d.ID]; ok {
				sug.IsDup = true
				sug.Reason = g.Reason
				sug.Suggest = "删除（" + g.Reason + "）"
				if g.Reason == "multi_version" {
					sug.Suggest = "删除（保留 " + g.Representative.Name + "）"
				}
				suggestions[d.ID] = sug
			}
		}
	}

	result := make([]Suggestion, 0, len(suggestions))
	for _, s := range suggestions {
		result = append(result, s)
	}

	c.JSON(http.StatusOK, gin.H{"data": result})
}

func (h *Handlers) GetCatalog(c *gin.Context) {
	search := c.Query("search")
	page, _ := strconv.Atoi(c.DefaultQuery("page", "1"))
	pageSize, _ := strconv.Atoi(c.DefaultQuery("page_size", "50"))
	entries, total, err := h.DB.GetCatalogEntries(search, page, pageSize)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"data":      entries,
		"total":     total,
		"page":      page,
		"page_size": pageSize,
	})
}

func (h *Handlers) UpdateCatalog(c *gin.Context) {
	id := c.Param("id")
	var entry core.CatalogEntry
	if err := c.ShouldBindJSON(&entry); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}
	entry.ID = id
	entry.MetaUpdatedAt = time.Now().UTC()
	if err := h.DB.UpdateCatalogEntry(entry); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "updated"})
}

func (h *Handlers) DeleteCatalog(c *gin.Context) {
	id := c.Param("id")
	if err := h.DB.DeleteCatalogEntry(id); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "deleted"})
}

type EnrichRequest struct {
	Provider    string `json:"provider"`
	SkipPrivate bool   `json:"skip_private"`
	Concurrency int    `json:"concurrency"`
}

func (h *Handlers) StartEnrich(c *gin.Context) {
	var req EnrichRequest
	if err := c.ShouldBindJSON(&req); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}
	if req.Concurrency < 1 {
		req.Concurrency = 3
	}

	// Load categories for AI guidance
	var catNames []string
	categoriesPath := filepath.Join(filepath.Dir(h.Cfg.DBPath), "categories.yaml")
	if data, err := os.ReadFile(categoriesPath); err == nil {
		var catData struct {
			Categories []struct {
				Name     string   `yaml:"name"`
				Keywords []string `yaml:"keywords"`
			} `yaml:"categories"`
		}
		if err := yaml.Unmarshal(data, &catData); err == nil {
			for _, cat := range catData.Categories {
				if len(cat.Keywords) > 0 {
					catNames = append(catNames, cat.Name+" ("+strings.Join(cat.Keywords, ",")+")")
				} else {
					catNames = append(catNames, cat.Name)
				}
			}
		}
	}

	go func() {
		records, _, err := h.DB.GetFileRecords("", "", "", 1, 1000000)
		if err != nil {
			h.Hub.Broadcast("enrich_error", gin.H{"error": err.Error()})
			return
		}

		// Build LLM enricher based on provider
		var llmEnricher ai.Enricher
		switch req.Provider {
		case "ollama":
			llmEnricher = ai.NewOllamaEnricher(h.Cfg.OllamaURL)
			if h.Cfg.OllamaModel != "" {
				llmEnricher.(*ai.OllamaEnricher).Model = h.Cfg.OllamaModel
			}
		case "openai":
			llmEnricher = ai.NewOpenAIEnricher(h.Cfg.OpenAIKey, h.Cfg.OpenAIBaseURL)
		case "claude":
			llmEnricher = ai.NewClaudeEnricher(h.Cfg.ClaudeKey, h.Cfg.ClaudeBaseURL)
		case "custom":
			e := ai.NewOpenAIEnricher(h.Cfg.CustomAIKey, h.Cfg.CustomAIURL)
			if h.Cfg.CustomAIModel != "" {
				e.Model = h.Cfg.CustomAIModel
			}
			llmEnricher = e
		case "offline":
			llmEnricher = nil
		default:
			llmEnricher = ai.NewOllamaEnricher("")
		}

		// Build offline enricher
		offlineDBPath := filepath.Join(filepath.Dir(h.Cfg.DBPath), "offline_db.sqlite")
		if _, err := os.Stat(offlineDBPath); os.IsNotExist(err) {
			slog.Info("离线知识库不存在，正在创建默认知识库", "path", offlineDBPath)
			if createErr := ai.CreateOfflineDB(offlineDBPath, ai.DefaultOfflineEntries()); createErr != nil {
				slog.Error("创建离线知识库失败", "error", createErr)
			}
		}
		offlineEnricher, _ := ai.NewOfflineEnricher(offlineDBPath)

		// Chain: offline first, LLM as fallback
		var enricher ai.Enricher
		if offlineEnricher != nil && llmEnricher != nil {
			enricher = ai.NewFallbackEnricher(offlineEnricher, llmEnricher)
		} else if offlineEnricher != nil {
			enricher = offlineEnricher
		} else if llmEnricher != nil {
			enricher = llmEnricher
		} else {
			h.Hub.Broadcast("enrich_error", gin.H{"error": "no enricher available"})
			return
		}
		// Filter out private/sensitive files
		privacy := core.NewPrivacyChecker(h.Cfg.PrivacyRules)
		var enrichable []core.FileRecord
		for _, r := range records {
			if privacy.ShouldSkip(r) || r.AISkip {
				continue
			}
			enrichable = append(enrichable, r)
		}

		slog.Info("enrich请求", "provider", req.Provider, "files", len(enrichable),
			"ollamaUrl", h.Cfg.OllamaURL, "openaiKey_set", h.Cfg.OpenAIKey != "",
			"claudeKey_set", h.Cfg.ClaudeKey != "")

		requests := make([]ai.EnrichRequest, len(enrichable))
		for i, r := range enrichable {
			requests[i] = ai.EnrichRequest{
				Name:      r.Name,
				Version:   r.Version,
				Extension: r.Extension,
				Category:  r.Category,
				FileSize:  r.FileSize,
			}
		}
		progressCh := make(chan ai.EnrichProgress, 16)
		go func() {
			for p := range progressCh {
				h.Hub.Broadcast("enrich_progress", gin.H{
					"total":   p.Total,
					"done":    p.Done,
					"current": p.Current,
				})
			}
		}()
		results, batchErr := ai.BatchEnrich(context.Background(), enricher, requests, catNames, req.Concurrency, progressCh)
		close(progressCh)

		slog.Info("enrich完成", "provider", req.Provider, "total", len(results), "error", batchErr)

		var savedCount, skippedCount int
		for i, result := range results {
			if result.Description == "" || result.Confidence == 0 {
				skippedCount++
				continue
			}
			entry := core.CatalogEntry{
				ID:                 fmt.Sprintf("cat_%s", enrichable[i].ID),
				Name:               enrichable[i].Name,
				Description:        result.Description,
				HomepageURL:        result.HomepageURL,
				DownloadURL:        result.DownloadURL,
				LatestVersion:      result.LatestVersion,
				License:            result.License,
				FunctionalCategory: result.FunctionalCategory,
				Tags:               result.Tags,
				AIConfidence:       result.Confidence,
				AIProvider:         result.Provider,
				NeedsReview:        result.NeedsReview,
				MetaUpdatedAt:      time.Now().UTC(),
			}
			h.DB.InsertCatalogEntry(entry)

			// Also update the file record with the functional category
			if result.FunctionalCategory != "" {
				h.DB.UpdateFileFunctionalCategory(enrichable[i].ID, result.FunctionalCategory)
			}
			savedCount++
		}
		slog.Info("enrich结果", "saved", savedCount, "skipped", skippedCount, "total", len(results))
		h.Hub.Broadcast("enrich_complete", gin.H{
			"provider": req.Provider,
			"total":    len(results),
			"saved":    savedCount,
			"skipped":  skippedCount,
		})
	}()
	c.JSON(http.StatusAccepted, gin.H{
		"status":   "enriching",
		"provider": req.Provider,
	})
}

func (h *Handlers) GetLogs(c *gin.Context) {
	sessionID := c.Query("session_id")
	action := c.Query("action")
	status := c.Query("status")
	q := c.Query("q")
	if search := c.Query("search"); search != "" {
		q = search
	}
	page, _ := strconv.Atoi(c.DefaultQuery("page", "1"))
	pageSize, _ := strconv.Atoi(c.DefaultQuery("page_size", "50"))
	logs, total, err := h.DB.GetOperationLogs(sessionID, action, status, q, page, pageSize)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{
		"data":      logs,
		"total":     total,
		"page":      page,
		"page_size": pageSize,
	})
}

func (h *Handlers) ExportCSV(c *gin.Context) {
	logs, _, err := h.DB.GetOperationLogs("", "", "", "", 1, 1000000)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	if len(logs) == 0 {
		c.JSON(http.StatusOK, gin.H{"message": "没有操作日志"})
		return
	}
	filename := fmt.Sprintf("filesweep_export_%s.csv", time.Now().Format("20060102_150405"))
	c.Header("Content-Disposition", "attachment; filename="+filename)
	c.Header("Content-Type", "text/csv; charset=utf-8")

	var buf bytes.Buffer
	w := csv.NewWriter(&buf)
	w.Write([]string{"timestamp", "operation", "source_path", "dest_path", "reason", "file_hash", "file_size", "status", "session_id", "can_revert"})
	for _, l := range logs {
		w.Write([]string{
			l.Timestamp.Format(time.RFC3339),
			string(l.Operation),
			l.SourcePath,
			l.DestPath,
			l.Reason,
			l.FileHash,
			strconv.FormatInt(l.FileSize, 10),
			l.Status,
			l.SessionID,
			strconv.FormatBool(l.CanRevert),
		})
	}
	w.Flush()
	c.Data(http.StatusOK, "text/csv; charset=utf-8", buf.Bytes())
}

// --- Category CRUD ---

func (h *Handlers) GetCategories(c *gin.Context) {
	cats, err := h.DB.GetCategories()
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	if cats == nil {
		cats = []db.Category{}
	}
	c.JSON(http.StatusOK, gin.H{"data": cats})
}

func (h *Handlers) CreateCategory(c *gin.Context) {
	var cat db.Category
	if err := c.ShouldBindJSON(&cat); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}
	if cat.ID == "" {
		cat.ID = "cat_" + uuid.New().String()[:8]
	}
	if err := h.DB.InsertCategory(cat); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "created", "id": cat.ID})
}

func (h *Handlers) UpdateCategory(c *gin.Context) {
	id := c.Param("id")
	var cat db.Category
	if err := c.ShouldBindJSON(&cat); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}
	cat.ID = id
	if err := h.DB.InsertCategory(cat); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "updated"})
}

func (h *Handlers) DeleteCategory(c *gin.Context) {
	id := c.Param("id")
	if err := h.DB.DeleteCategory(id); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "deleted"})
}

// --- Revert ---

func (h *Handlers) RevertOperation(c *gin.Context) {
	idStr := c.Param("id")
	id, err := strconv.ParseInt(idStr, 10, 64)
	if err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "无效的日志 ID"})
		return
	}
	logEntry, err := h.DB.GetOperationLogByID(id)
	if err != nil {
		c.JSON(http.StatusNotFound, gin.H{"error": "操作记录不存在"})
		return
	}
	if !logEntry.CanRevert {
		c.JSON(http.StatusBadRequest, gin.H{"error": "该操作不可回退"})
		return
	}

	switch logEntry.Operation {
	case "MOVE":
		if logEntry.DestPath == "" {
			c.JSON(http.StatusBadRequest, gin.H{"error": "缺少目标路径"})
			return
		}
		if err := core.RevertMove(logEntry.SourcePath, logEntry.DestPath); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "回退失败: " + err.Error()})
			return
		}
	case "DELETE":
		trashPath := core.FindInTrash(logEntry.SourcePath)
		if trashPath == "" {
			c.JSON(http.StatusNotFound, gin.H{"error": "回收站中未找到该文件"})
			return
		}
		if err := core.RevertFromTrash(trashPath, logEntry.SourcePath); err != nil {
			c.JSON(http.StatusInternalServerError, gin.H{"error": "回退失败: " + err.Error()})
			return
		}
	default:
		c.JSON(http.StatusBadRequest, gin.H{"error": "不支持回退该操作类型"})
		return
	}

	_ = h.DB.MarkLogReverted(id)
	c.JSON(http.StatusOK, gin.H{"status": "reverted"})
}

func (h *Handlers) BatchRevert(c *gin.Context) {
	var req struct {
		IDs []int64 `json:"ids"`
	}
	if err := c.ShouldBindJSON(&req); err != nil || len(req.IDs) == 0 {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请提供要回退的日志 ID 列表"})
		return
	}

	type revertResult struct {
		ID    int64  `json:"id"`
		Ok    bool   `json:"ok"`
		Error string `json:"error,omitempty"`
	}
	var results []revertResult

	for _, id := range req.IDs {
		logEntry, err := h.DB.GetOperationLogByID(id)
		if err != nil {
			results = append(results, revertResult{ID: id, Error: "操作记录不存在"})
			continue
		}
		if !logEntry.CanRevert {
			results = append(results, revertResult{ID: id, Error: "该操作不可回退"})
			continue
		}

		var opErr error
		switch logEntry.Operation {
		case "MOVE":
			if logEntry.DestPath == "" {
				opErr = fmt.Errorf("缺少目标路径，无法回退")
			} else {
				opErr = core.RevertMove(logEntry.SourcePath, logEntry.DestPath)
			}
		case "DELETE":
			trashPath := core.FindInTrash(logEntry.SourcePath)
			if trashPath == "" {
				opErr = fmt.Errorf("回收站中未找到该文件（可能已被清空或文件已被改名）")
			} else {
				opErr = core.RevertFromTrash(trashPath, logEntry.SourcePath)
			}
		default:
			opErr = fmt.Errorf("不支持回退该操作类型")
		}

		if opErr != nil {
			results = append(results, revertResult{ID: id, Error: opErr.Error()})
		} else {
			_ = h.DB.MarkLogReverted(id)
			results = append(results, revertResult{ID: id, Ok: true})
		}
	}

	c.JSON(http.StatusOK, gin.H{"results": results})
}

// --- Rules (rules.yaml) ---

func (h *Handlers) GetRules(c *gin.Context) {
	rulesPath := ensureRulesPath(h.Cfg)
	classifier, err := core.NewClassifier(rulesPath)
	if err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "读取规则文件失败: " + err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"data": classifier.Rules.Categories})
}

func (h *Handlers) UpdateRules(c *gin.Context) {
	var categories []core.CategoryRule
	if err := c.ShouldBindJSON(&categories); err != nil {
		c.JSON(http.StatusBadRequest, gin.H{"error": "请求参数无效"})
		return
	}
	rulesPath := ensureRulesPath(h.Cfg)
	cfg := core.RulesConfig{Categories: categories}
	if err := core.SaveRules(rulesPath, cfg); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "保存规则失败: " + err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "updated"})
}

func (h *Handlers) ResetDB(c *gin.Context) {
	if err := h.DB.Reset(); err != nil {
		c.JSON(http.StatusInternalServerError, gin.H{"error": "重置数据库失败: " + err.Error()})
		return
	}
	c.JSON(http.StatusOK, gin.H{"status": "reset"})
}

func ensureRulesPath(cfg *config.Config) string {
	if cfg.RulesPath == "" {
		cfg.RulesPath = filepath.Join("config", "rules.yaml")
	}
	dir := filepath.Dir(cfg.RulesPath)
	os.MkdirAll(dir, 0755)
	if _, err := os.Stat(cfg.RulesPath); os.IsNotExist(err) {
		core.SaveRules(cfg.RulesPath, core.DefaultRules())
	}
	return cfg.RulesPath
}
