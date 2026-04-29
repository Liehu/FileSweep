package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"sync"
	"sync/atomic"
)

type EnrichRequest struct {
	Name          string   `json:"name"`
	Version       string   `json:"version"`
	Extension     string   `json:"extension"`
	Category      string   `json:"category"`
	FileSize      int64    `json:"file_size"`
	AvailableTags []string `json:"available_tags,omitempty"`
}

type EnrichResult struct {
	Description        string   `json:"description"`
	HomepageURL        string   `json:"homepage_url"`
	DownloadURL        string   `json:"download_url"`
	LatestVersion      string   `json:"latest_version"`
	License            string   `json:"license"`
	FunctionalCategory string   `json:"functional_category"`
	Tags               []string `json:"tags"`
	Confidence         float64  `json:"confidence"`
	NeedsReview        bool     `json:"needs_review"`
	Provider           string   `json:"provider"`
}

type Enricher interface {
	Enrich(ctx context.Context, req EnrichRequest, categories []string) (EnrichResult, error)
	Name() string
}

const systemPrompt = `你是软件元数据分类专家。根据文件名、版本、扩展名等信息，从预定义分类体系中返回精确的软件元数据。

【严格规则 - 必须遵守】
1. functional_category：必须从用户消息的"可选功能分类"列表中精确选择一个。绝不许自创分类名称。若无匹配项，选择"其他"。
2. tags：必须从用户消息的"可选标签"列表中选择，最多5个。若无合适标签，选择"utility"。绝不许自创标签。
3. description：使用中文，≤120字，简述软件核心功能。
4. homepage_url / download_url：仅填写确定的真实官方URL，不确定则留空。禁止编造URL。
5. confidence：根据你对文件名的确信程度给出0.0-1.0的浮点数，不确定时填0.3。

【输出格式】
返回纯JSON对象，包含以下字段：
- description (string, 中文, ≤120字)
- homepage_url (string, 官网地址)
- download_url (string, 下载页面地址，非直接文件链接)
- latest_version (string, 你所知的最新版本号)
- license (string, 如MIT/GPLv2/Commercial/Freeware/Proprietary)
- functional_category (string, 必须精确匹配分类列表中的某一项)
- tags (string[], ≤5个, 必须从标签列表中选择)
- confidence (float 0.0-1.0)

仅返回合法JSON，不要markdown代码块，不要额外说明文字。`

func ParseEnrichResponse(data []byte, provider string) (EnrichResult, error) {
	var raw struct {
		Description        string   `json:"description"`
		HomepageURL        string   `json:"homepage_url"`
		DownloadURL        string   `json:"download_url"`
		LatestVersion      string   `json:"latest_version"`
		License            string   `json:"license"`
		FunctionalCategory string   `json:"functional_category"`
		Tags               []string `json:"tags"`
		Confidence         float64  `json:"confidence"`
	}

	if err := json.Unmarshal(data, &raw); err != nil {
		return EnrichResult{}, fmt.Errorf("解析 AI 响应失败: %w", err)
	}

	result := EnrichResult{
		Description:        raw.Description,
		HomepageURL:        raw.HomepageURL,
		DownloadURL:        raw.DownloadURL,
		LatestVersion:      raw.LatestVersion,
		License:            raw.License,
		FunctionalCategory: raw.FunctionalCategory,
		Tags:               raw.Tags,
		Confidence:         raw.Confidence,
		Provider:           provider,
		NeedsReview:        raw.Confidence < 0.6,
	}

	if result.Confidence == 0 {
		result.Confidence = 0.3
		result.NeedsReview = true
	}

	return result, nil
}


type EnrichProgress struct {
	Total   int            `json:"total"`
	Done    int            `json:"done"`
	Current string         `json:"current"`
	Stage   string         `json:"stage"`
	Results []EnrichResult `json:"results,omitempty"`
}

func BatchEnrich(ctx context.Context, enricher Enricher, requests []EnrichRequest, categories []string, concurrency int, progressCh chan<- EnrichProgress) ([]EnrichResult, error) {
	if concurrency < 1 {
		concurrency = 5
	}

	total := len(requests)
	results := make([]EnrichResult, total)
	var doneCount atomic.Int64
	var wg sync.WaitGroup
	sem := make(chan struct{}, concurrency)

	progressCh <- EnrichProgress{Total: total, Stage: "enriching"}

	for i, req := range requests {
		// Check cancellation first, then acquire semaphore
		if ctx.Err() != nil {
			break
		}
		select {
		case <-ctx.Done():
			break
		case sem <- struct{}{}:
		}

		wg.Add(1)
		go func(idx int, r EnrichRequest) {
			defer wg.Done()
			defer func() { <-sem }()

			result, err := enricher.Enrich(ctx, r, categories)
			if err != nil {
				slog.Warn("enrich失败", "file", r.Name, "provider", enricher.Name(), "error", err)
				result = EnrichResult{
					Confidence:  0,
					NeedsReview: true,
					Provider:    enricher.Name(),
					Description: err.Error(),
				}
			}
			results[idx] = result
			current := int(doneCount.Add(1))

			if progressCh != nil {
				progressCh <- EnrichProgress{
					Total:   total,
					Done:    current,
					Current: r.Name,
					Stage:   "enriching",
				}
			}
		}(i, req)
	}

	wg.Wait()

	if progressCh != nil {
		progressCh <- EnrichProgress{Total: total, Done: total, Stage: "complete"}
	}

	if ctx.Err() != nil {
		var completed []EnrichResult
		for _, r := range results {
			if r.Provider != "" {
				completed = append(completed, r)
			}
		}
		return completed, ctx.Err()
	}

	return results, nil
}
