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

type OpenAIEnricher struct {
	APIKey  string
	BaseURL string
	Model   string
	client  *http.Client
}

func NewOpenAIEnricher(apiKey, baseURL string) *OpenAIEnricher {
	url := strings.TrimRight(baseURL, "/")
	if url == "" {
		url = "https://api.openai.com/v1"
	}
	return &OpenAIEnricher{
		APIKey:  apiKey,
		BaseURL: url,
		Model:   "gpt-4o",
		client:  &http.Client{Timeout: 30 * time.Second},
	}
}

func (o *OpenAIEnricher) Name() string {
	return "openai"
}

func (o *OpenAIEnricher) Enrich(ctx context.Context, req EnrichRequest) (EnrichResult, error) {
	if o.APIKey == "" {
		return EnrichResult{}, fmt.Errorf("OpenAI API key 未设置")
	}

	userMsg := fmt.Sprintf("File: %s, Version: %s, Extension: %s, Category: %s",
		req.Name, req.Version, req.Extension, req.Category)

	body := map[string]any{
		"model": o.Model,
		"messages": []map[string]string{
			{"role": "system", "content": systemPrompt},
			{"role": "user", "content": userMsg},
		},
		"temperature": 0.3,
	}
	jsonBody, _ := json.Marshal(body)

	httpReq, err := http.NewRequestWithContext(ctx, "POST", o.BaseURL+"/chat/completions", bytes.NewReader(jsonBody))
	if err != nil {
		return EnrichResult{}, err
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Header.Set("Authorization", "Bearer "+o.APIKey)

	resp, err := o.client.Do(httpReq)
	if err != nil {
		return EnrichResult{}, fmt.Errorf("OpenAI API 调用失败: %w", err)
	}
	defer resp.Body.Close()

	respBody, err := io.ReadAll(resp.Body)
	if err != nil {
		return EnrichResult{}, err
	}

	if resp.StatusCode != http.StatusOK {
		return EnrichResult{}, fmt.Errorf("OpenAI API 返回错误 %d: %s", resp.StatusCode, string(respBody))
	}

	var chatResp struct {
		Choices []struct {
			Message struct {
				Content string `json:"content"`
			} `json:"message"`
		} `json:"choices"`
	}
	if err := json.Unmarshal(respBody, &chatResp); err != nil {
		return EnrichResult{}, err
	}

	if len(chatResp.Choices) == 0 {
		return EnrichResult{}, fmt.Errorf("OpenAI 返回空响应")
	}

	content := strings.TrimSpace(chatResp.Choices[0].Message.Content)
	content = strings.TrimPrefix(content, "```json")
	content = strings.TrimPrefix(content, "```")
	content = strings.TrimSuffix(content, "```")
	content = strings.TrimSpace(content)

	return ParseEnrichResponse([]byte(content), "openai")
}
