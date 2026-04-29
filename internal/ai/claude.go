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

type ClaudeEnricher struct {
	APIKey  string
	BaseURL string
	Model   string
	client  *http.Client
}

func NewClaudeEnricher(apiKey, baseURL string) *ClaudeEnricher {
	url := strings.TrimRight(baseURL, "/")
	if url == "" {
		url = "https://api.anthropic.com"
	}
	return &ClaudeEnricher{
		APIKey:  apiKey,
		BaseURL: url,
		Model:   "claude-sonnet-4-20250514",
		client:  &http.Client{Timeout: 30 * time.Second},
	}
}

func (c *ClaudeEnricher) Name() string {
	return "claude"
}

func (c *ClaudeEnricher) Enrich(ctx context.Context, req EnrichRequest, categories []string) (EnrichResult, error) {
	if c.APIKey == "" {
		return EnrichResult{}, fmt.Errorf("Claude API key 未设置")
	}

	userMsg := fmt.Sprintf("File: %s, Version: %s, Extension: %s, Category: %s",
		req.Name, req.Version, req.Extension, req.Category)
	if len(categories) > 0 {
		userMsg += fmt.Sprintf(", Optional Functional Categories: %s", strings.Join(categories, ", "))
	}

	body := map[string]any{
		"model":      c.Model,
		"max_tokens": 1024,
		"system":     systemPrompt,
		"messages": []map[string]string{
			{"role": "user", "content": userMsg},
		},
	}
	jsonBody, _ := json.Marshal(body)

	httpReq, err := http.NewRequestWithContext(ctx, "POST", c.BaseURL+"/v1/messages", bytes.NewReader(jsonBody))
	if err != nil {
		return EnrichResult{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("x-api-key", c.APIKey)
	httpReq.Header.Set("anthropic-version", "2023-06-01")

	resp, err := c.client.Do(httpReq)
	if err != nil {
		return EnrichResult{}, fmt.Errorf("Claude API 调用失败: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return EnrichResult{}, err
	}

	if resp.StatusCode != http.StatusOK {
		return EnrichResult{}, fmt.Errorf("Claude API 返回错误 %d: %s", resp.StatusCode, string(respBody))
	}

	var msgResp struct {
		Content []struct {
			Text string `json:"text"`
		} `json:"content"`
	}
	if err := json.Unmarshal(respBody, &msgResp); err != nil {
		return EnrichResult{}, err
	}

	if len(msgResp.Content) == 0 {
		return EnrichResult{}, fmt.Errorf("Claude 返回空响应")
	}

	content := strings.TrimSpace(msgResp.Content[0].Text)
	content = strings.TrimPrefix(content, "```json")
	content = strings.TrimPrefix(content, "```")
	content = strings.TrimSuffix(content, "```")
	content = strings.TrimSpace(content)

	return ParseEnrichResponse([]byte(content), "claude")
}
