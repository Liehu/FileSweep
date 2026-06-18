use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::ai::enricher::{
    build_system_prompt, build_user_message, default_enrich_result, parse_enrich_response,
    strip_markdown_fences, EnrichRequest, EnrichResult, Enricher,
};

// ────────────────── OpenAI Enricher ──────────────────

pub struct OpenAIEnricher {
    api_key: String,
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OpenAIEnricher {
    pub fn new(api_key: &str, base_url: &str) -> Self {
        Self {
            api_key: api_key.to_string(),
            base_url: if base_url.is_empty() {
                "https://api.openai.com/v1".to_string()
            } else {
                base_url.to_string()
            },
            model: "gpt-4o".to_string(),
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

    /// 执行一次 API 调用，最多重试 3 次（仅在 429 Too Many Requests 时重试）。
    async fn call_api(
        client: &reqwest::Client,
        base_url: &str,
        api_key: &str,
        model: &str,
        system_prompt: &str,
        user_msg: &str,
        name: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_msg },
            ],
            "temperature": 0.3,
            "max_tokens": 500,
        });

        let mut last_err = String::new();

        for attempt in 0..3 {
            let resp = client
                .post(&url)
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            match resp {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let json: serde_json::Value = response.json().await?;
                        // 提取 choices[0].message.content
                        if let Some(content) = json
                            .pointer("/choices/0/message/content")
                            .and_then(|v| v.as_str())
                        {
                            return Ok(content.to_string());
                        }
                        let err_msg = format!("OpenAI unexpected response format for '{}'", name);
                        log::warn!("{}", err_msg);
                        return Err(err_msg.into());
                    } else if status.as_u16() == 429 && attempt < 2 {
                        // 速率限制，等待后重试
                        let retry_after = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(5);
                        log::warn!(
                            "OpenAI 429 for '{}', attempt {}/3, retry after {}s",
                            name,
                            attempt + 1,
                            retry_after
                        );
                        tokio::time::sleep(Duration::from_secs(retry_after)).await;
                        last_err = "429 rate limited".into();
                        continue;
                    } else {
                        let status_text = response.text().await.unwrap_or_default();
                        last_err = format!("OpenAI {} error for '{}': {}", status, name, status_text);
                        log::warn!("{}", last_err);
                        return Err(last_err.into());
                    }
                }
                Err(e) => {
                    last_err = format!("OpenAI request failed for '{}': {}", name, e);
                    log::warn!("{}", last_err);
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                    return Err(last_err.into());
                }
            }
        }

        Err(format!("OpenAI exhausted retries for '{}': {}", name, last_err).into())
    }
}

impl Enricher for OpenAIEnricher {
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
            let raw = Self::call_api(
                &client,
                &base_url,
                &api_key,
                &model,
                &system_prompt,
                &user_msg,
                &name,
            )
            .await?;

            Ok(parse_enrich_response(&raw, "openai", &name))
        })
    }

    fn name(&self) -> &str {
        "openai"
    }
}
