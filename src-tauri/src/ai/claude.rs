use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::ai::enricher::{
    build_system_prompt, build_user_message, default_enrich_result, parse_enrich_response,
    EnrichRequest, EnrichResult, Enricher,
};

// ────────────────── Claude Enricher ──────────────────

pub struct ClaudeEnricher {
    api_key: String,
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl ClaudeEnricher {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: if base_url.is_empty() {
                "https://api.anthropic.com".to_string()
            } else {
                base_url.to_string()
            },
            model: "claude-sonnet-4-20250514".to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }
}

impl Enricher for ClaudeEnricher {
    fn enrich(
        &self,
        req: EnrichRequest,
        categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<EnrichResult, Box<dyn std::error::Error + Send + Sync>>> + Send + '_>>
    {
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();
        let client = self.client.clone();
        let name = req.name.clone();
        let tags = req.available_tags.clone().unwrap_or_default();
        let system_prompt = build_system_prompt(&categories, &tags);
        let user_msg = build_user_message(&req);

        Box::pin(async move {
            let url = format!("{}/v1/messages", base_url.trim_end_matches('/'));
            let body = serde_json::json!({
                "model": model,
                "max_tokens": 500,
                "system": system_prompt,
                "messages": [
                    { "role": "user", "content": user_msg },
                ],
            });

            let resp = client
                .post(&url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                    format!("Claude request failed for '{}': {}", name, e).into()
                })?;

            let status = resp.status();
            if !status.is_success() {
                let status_text = resp.text().await.unwrap_or_default();
                let err_msg = format!("Claude {} error for '{}': {}", status, name, status_text);
                log::warn!("{}", err_msg);
                return Err(err_msg.into());
            }

            let json: serde_json::Value = resp.json().await?;
            // Claude 响应格式: { "content": [{ "type": "text", "text": "..." }] }
            let raw = json
                .pointer("/content/0/text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    let err_msg = format!("Claude unexpected response format for '{}'", name);
                    log::warn!("{}", err_msg);
                    err_msg
                })?;

            Ok(parse_enrich_response(raw, "claude", &name))
        })
    }

    fn name(&self) -> &str {
        "claude"
    }
}
