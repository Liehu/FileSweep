package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

type OllamaEnricher struct {
	BaseURL string
	Model   string
	client  *http.Client
}

func NewOllamaEnricher(baseURL string) *OllamaEnricher {
	url := strings.TrimRight(baseURL, "/")
	if url == "" {
		url = "http://localhost:11434"
	}
	return &OllamaEnricher{
		BaseURL: url,
		Model:   "",
		client:  &http.Client{Timeout: 120 * time.Second},
	}
}

func (o *OllamaEnricher) Name() string {
	return "ollama"
}

func (o *OllamaEnricher) Enrich(ctx context.Context, req EnrichRequest, categories []string) (EnrichResult, error) {
	model := o.Model
	if model == "" {
		model = "llama3.1:8b"
	}

	userMsg := fmt.Sprintf("File: %s, Version: %s, Extension: %s, Category: %s",
		req.Name, req.Version, req.Extension, req.Category)
	if len(categories) > 0 {
		userMsg += fmt.Sprintf("\n可选功能分类: %s", strings.Join(categories, ", "))
	}
	if len(req.AvailableTags) > 0 {
		userMsg += fmt.Sprintf("\n可选标签: %s", strings.Join(req.AvailableTags, ", "))
	}

	body := map[string]any{
		"model":  model,
		"system": systemPrompt,
		"prompt": userMsg,
		"stream": false,
		"format": "json",
	}
	jsonBody, _ := json.Marshal(body)

	var lastErr error
	for attempt := 0; attempt < 3; attempt++ {
		if attempt > 0 {
			wait := time.Duration(attempt*5) * time.Second
			select {
			case <-time.After(wait):
			case <-ctx.Done():
				return EnrichResult{}, ctx.Err()
			}
		}

		httpReq, err := http.NewRequestWithContext(ctx, "POST", o.BaseURL+"/api/generate", bytes.NewReader(jsonBody))
		if err != nil {
			return EnrichResult{}, err
		}
		httpReq.Header.Set("Content-Type", "application/json")

		resp, err := o.client.Do(httpReq)
		if err != nil {
			lastErr = fmt.Errorf("Ollama 调用失败: %w", err)
			continue
		}

		respBody, err := io.ReadAll(resp.Body)
		resp.Body.Close()
		if err != nil {
			lastErr = err
			continue
		}

		if resp.StatusCode == 404 {
			return EnrichResult{}, fmt.Errorf("Ollama 模型 '%s' 不存在，请先 ollama pull %s", model, model)
		}

		if resp.StatusCode != http.StatusOK {
			lastErr = fmt.Errorf("Ollama 返回错误 %d: %s", resp.StatusCode, string(respBody))
			continue
		}

		var genResp struct {
			Response string `json:"response"`
		}
		if err := json.Unmarshal(respBody, &genResp); err != nil {
			return EnrichResult{}, err
		}

		content := strings.TrimSpace(genResp.Response)
		return ParseEnrichResponse([]byte(content), "ollama")
	}

	return EnrichResult{}, lastErr
}
