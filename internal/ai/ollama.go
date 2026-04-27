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
		Model:   "llama3.1:8b",
		client:  &http.Client{Timeout: 60 * time.Second},
	}
}

func (o *OllamaEnricher) Name() string {
	return "ollama"
}

func (o *OllamaEnricher) Enrich(ctx context.Context, req EnrichRequest) (EnrichResult, error) {
	userMsg := fmt.Sprintf("File: %s, Version: %s, Extension: %s, Category: %s",
		req.Name, req.Version, req.Extension, req.Category)

	body := map[string]any{
		"model":  o.Model,
		"system": systemPrompt,
		"prompt": userMsg,
		"stream": false,
		"format": "json",
	}
	jsonBody, _ := json.Marshal(body)

	httpReq, err := http.NewRequestWithContext(ctx, "POST", o.BaseURL+"/api/generate", bytes.NewReader(jsonBody))
	if err != nil {
		return EnrichResult{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")

	resp, err := o.client.Do(httpReq)
	if err != nil {
		return EnrichResult{}, fmt.Errorf("Ollama 调用失败: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return EnrichResult{}, err
	}

	if resp.StatusCode != http.StatusOK {
		return EnrichResult{}, fmt.Errorf("Ollama 返回错误 %d: %s", resp.StatusCode, string(respBody))
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
