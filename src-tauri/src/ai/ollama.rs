use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::ai::enricher::{
    build_system_prompt, build_user_message, default_enrich_result, parse_enrich_response,
    EnrichRequest, EnrichResult, Enricher,
};

// ────────────────── Ollama Enricher ──────────────────

pub struct OllamaEnricher {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaEnricher {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: if base_url.is_empty() {
                "http://localhost:11434".to_string()
            } else {
                base_url.to_string()
            },
            model: "llama3.1:8b".to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .connect_timeout(Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// 调用 Ollama 本地 API，最多重试 3 次，每次间隔 5 秒。
    async fn call_api(
        client: &reqwest::Client,
        base_url: &str,
        model: &str,
        system_prompt: &str,
        user_msg: &str,
        name: &str,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/api/generate", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "prompt": format!("{}\n\nUser: {}", system_prompt, user_msg),
            "stream": false,
            "format": "json",
        });

        let mut last_err = String::new();

        for attempt in 0..3 {
            let resp = client.post(&url).json(&body).send().await;

            match resp {
                Ok(response) => {
                    let status = response.status();
                    if status.is_success() {
                        let json: serde_json::Value = response.json().await?;
                        // Ollama 响应格式: { "response": "..." }
                        if let Some(response_text) = json.get("response").and_then(|v| v.as_str()) {
                            return Ok(response_text.to_string());
                        }
                        let err_msg =
                            format!("Ollama unexpected response format for '{}'", name);
                        log::warn!("{}", err_msg);
                        return Err(err_msg.into());
                    } else {
                        let status_text = response.text().await.unwrap_or_default();
                        last_err = format!("Ollama {} error for '{}': {}", status, name, status_text);
                        log::warn!("{}", last_err);
                        if attempt < 2 {
                            tokio::time::sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                        return Err(last_err.into());
                    }
                }
                Err(e) => {
                    // 连接失败（Ollama 未运行等）
                    last_err = format!("Ollama request failed for '{}': {}", name, e);
                    log::warn!("{}", last_err);
                    if attempt < 2 {
                        tokio::time::sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                    return Err(last_err.into());
                }
            }
        }

        Err(format!("Ollama exhausted retries for '{}': {}", name, last_err).into())
    }
}

impl Enricher for OllamaEnricher {
    fn enrich(
        &self,
        req: EnrichRequest,
        categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<EnrichResult, Box<dyn std::error::Error + Send + Sync>>> + Send + '_>>
    {
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
                &model,
                &system_prompt,
                &user_msg,
                &name,
            )
            .await?;

            Ok(parse_enrich_response(&raw, "ollama", &name))
        })
    }

    fn name(&self) -> &str {
        "ollama"
    }
}
