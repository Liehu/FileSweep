package ai

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"sync/atomic"
)

type EnrichRequest struct {
	Name      string `json:"name"`
	Version   string `json:"version"`
	Extension string `json:"extension"`
	Category  string `json:"category"`
	FileSize  int64  `json:"file_size"`
}

type EnrichResult struct {
	Description   string   `json:"description"`
	HomepageURL   string   `json:"homepage_url"`
	DownloadURL   string   `json:"download_url"`
	LatestVersion string   `json:"latest_version"`
	License       string   `json:"license"`
	Tags          []string `json:"tags"`
	Confidence    float64  `json:"confidence"`
	NeedsReview   bool     `json:"needs_review"`
	Provider      string   `json:"provider"`
}

type Enricher interface {
	Enrich(ctx context.Context, req EnrichRequest) (EnrichResult, error)
	Name() string
}

const systemPrompt = `You are a software metadata expert. Given a file name, version, and category, return ONLY a JSON object with these exact fields: description (string, ≤120 chars, Chinese preferred), homepage_url (string, official website only), download_url (string, download page URL, not direct file link), latest_version (string, your best knowledge), license (string, e.g. MIT/GPLv2/Commercial), tags (array of strings, ≤5 tags), confidence (float 0.0-1.0, your certainty). If unsure about any field, use empty string or 0.3 confidence. NEVER fabricate URLs. Return pure JSON only, no markdown fences.`

func ParseEnrichResponse(data []byte, provider string) (EnrichResult, error) {
	var raw struct {
		Description   string   `json:"description"`
		HomepageURL   string   `json:"homepage_url"`
		DownloadURL   string   `json:"download_url"`
		LatestVersion string   `json:"latest_version"`
		License       string   `json:"license"`
		Tags          []string `json:"tags"`
		Confidence    float64  `json:"confidence"`
	}

	if err := json.Unmarshal(data, &raw); err != nil {
		return EnrichResult{}, fmt.Errorf("解析 AI 响应失败: %w", err)
	}

	result := EnrichResult{
		Description:   raw.Description,
		HomepageURL:   raw.HomepageURL,
		DownloadURL:   raw.DownloadURL,
		LatestVersion: raw.LatestVersion,
		License:       raw.License,
		Tags:          raw.Tags,
		Confidence:    raw.Confidence,
		Provider:      provider,
		NeedsReview:   raw.Confidence < 0.6,
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

func BatchEnrich(ctx context.Context, enricher Enricher, requests []EnrichRequest, concurrency int, progressCh chan<- EnrichProgress) ([]EnrichResult, error) {
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

			result, err := enricher.Enrich(ctx, r)
			if err != nil {
				result = EnrichResult{
					Confidence:  0,
					NeedsReview: true,
					Provider:    enricher.Name(),
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
