use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use crate::ai::enricher::{
    build_batch_system_prompt, build_batch_user_message, build_system_prompt,
    build_user_message, default_enrich_result, parse_batch_response, parse_enrich_response,
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

    /// 测试连通性+认证：发一个极简 ping 请求（不重试，快速失败）。
    ///
    /// 成功返回 `Ok(model)`，失败返回 `Err(服务端原始报错)`——
    /// HTTP 状态码 + response body（如 "401 Unauthorized: {\"error\":{\"message\":\"invalid api key\"}}"）。
    /// 不吞错误，把服务端原文透给前端用于诊断。
    pub async fn test_connection(&self) -> Result<String, String> {
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": "reply ok" },
                { "role": "user", "content": "ping" },
            ],
            "max_tokens": 200,  // reasoning 模型需要思考预算，10 不够会 content=null
            "reasoning": { "enabled": false },  // 同 call_api：降低 reasoning 消耗，避免 content=null
        });

        let start = std::time::Instant::now();
        let resp = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("网络请求失败: {}", e))?;

        let status = resp.status();
        let latency_ms = start.elapsed().as_millis();

        if status.is_success() {
            // 成功：返回 model + 延迟（延迟通过错误通道不便携带复杂结构，这里塞进 model 字符串）
            Ok(format!("{}|{}ms", self.model, latency_ms))
        } else {
            // 失败：返回 HTTP 状态码 + 服务端 body 原文
            let body_text = resp.text().await.unwrap_or_default();
            Err(format!("HTTP {}: {}", status.as_u16(), body_text))
        }
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
        max_tokens: u32,
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        let body = serde_json::json!({
            "model": model,
            "messages": [
                { "role": "system", "content": system_prompt },
                { "role": "user", "content": user_msg },
            ],
            "temperature": 0.3,
            "max_tokens": max_tokens,
            // OpenRouter reasoning 模型（hy3/qwen3-next 等）会把 max_tokens 耗在 reasoning
            // 上导致 content=null（实测 1500 token 全被 reasoning 吃光）。
            // enabled:false 真正降低模型推理消耗（exclude:true 只是不返回文本，仍消耗预算）。
            // 实测：enabled:false 后 reasoning_tokens 从 1500 降到 ~200，content 正常返回。
            // 对非 reasoning 模型/原生 OpenAI 无害（忽略未知参数）。
            "reasoning": { "enabled": false },
        });

        let mut last_err = String::new();
        // 重试上限 5 次（OpenRouter free 模型限流窗口可能较长，3 次不够）。
        const MAX_ATTEMPTS: u32 = 5;

        for attempt in 0..MAX_ATTEMPTS {
            // 中断检查：收到信号后立即停止重试（避免中断时还在傻等退避）
            if crate::commands::enrich::is_enrich_cancelled() {
                return Err("enrich cancelled".into());
            }

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
                        // content 为 null/缺失：常见于 reasoning 模型 max_tokens 不足
                        // （reasoning 吃光预算，content 没生成）。记录原始响应便于诊断。
                        let body_preview = serde_json::to_string(&json)
                            .unwrap_or_default()
                            .chars()
                            .take(500)
                            .collect::<String>();
                        log::warn!(
                            "OpenAI '{}' content 缺失（可能 reasoning 模型 max_tokens 不足），原始响应: {}",
                            name,
                            body_preview
                        );
                        return Err(
                            format!("content 缺失（reasoning 模型 max_tokens 不足？）: {}", name).into(),
                        );
                    } else if status.as_u16() == 429 && attempt + 1 < MAX_ATTEMPTS {
                        // 速率限制，指数退避：5/10/20/40/60s（取 retry-after 与退避的较大值）
                        let backoff = match attempt {
                            0 => 5,
                            1 => 10,
                            2 => 20,
                            3 => 40,
                            _ => 60,
                        };
                        let retry_after = response
                            .headers()
                            .get("retry-after")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(0);
                        let wait = retry_after.max(backoff);
                        log::warn!(
                            "OpenAI 429 for '{}', attempt {}/{}, backoff {}s (retry-after {}s)",
                            name,
                            attempt + 1,
                            MAX_ATTEMPTS,
                            wait,
                            retry_after
                        );
                        tokio::time::sleep(Duration::from_secs(wait)).await;
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
                    if attempt + 1 < MAX_ATTEMPTS {
                        // 网络错误也用指数退避（2/4/8s），避免快速重试加剧问题
                        let backoff = 2u64 * 2u64.pow(attempt);
                        tokio::time::sleep(Duration::from_secs(backoff)).await;
                        continue;
                    }
                    return Err(last_err.into());
                }
            }
        }

        Err(format!("OpenAI exhausted {} retries for '{}': {}", MAX_ATTEMPTS, name, last_err).into())
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
                1500,  // 单文件：reasoning 模型需思考预算，500 不够会 content=null
            )
            .await?;

            Ok(parse_enrich_response(&raw, "openai", &name))
        })
    }

    /// 批量补全：一次 HTTP 请求处理整个 batch，显著降低请求数（479 文件 → ~24 批）。
    ///
    /// max_tokens = min(8192, 400 * batch_len)：reasoning 模型需思考预算，每文件 ~400 token
    /// （reasoning + JSON 输出），上限 8192。若超限或 JSON 解析失败 → 降级单文件逐个调。
    fn enrich_batch(
        &self,
        batch: Vec<(usize, EnrichRequest)>,
        categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Vec<(usize, EnrichResult)>> + Send + '_>> {
        let api_key = self.api_key.clone();
        let base_url = self.base_url.clone();
        let model = self.model.clone();
        let client = self.client.clone();
        let batch_len = batch.len();
        let tags = batch
            .first()
            .and_then(|(_, r)| r.available_tags.clone())
            .unwrap_or_default();
        let system_prompt = build_batch_system_prompt(&categories, &tags);
        let user_msg = build_batch_user_message(&batch);
        // max_tokens：reasoning 模型每文件 ~400 token（思考 + JSON 输出），上限 8192
        let max_tokens = std::cmp::min(8192, 400 * batch_len as u32).max(1500);

        Box::pin(async move {
            log::info!(
                "OpenAIEnricher batch: {} files, max_tokens={}",
                batch_len,
                max_tokens
            );
            let raw = Self::call_api(
                &client,
                &base_url,
                &api_key,
                &model,
                &system_prompt,
                &user_msg,
                "batch",
                max_tokens,
            )
            .await;

            let parsed = match raw {
                Ok(r) => parse_batch_response(&r, "openai", batch_len),
                Err(e) => {
                    log::warn!("OpenAIEnricher batch call failed ({}), 降级单文件: {}", batch_len, e);
                    Vec::new()
                }
            };

            // 降级判定：解析返回空，或 default（空 description）占比 >50%
            // → 模型没正常处理批次，拆成单文件逐个调
            let empty_count = parsed.iter().filter(|(_, r)| r.description.is_empty()).count();
            let need_fallback = parsed.is_empty()
                || (!parsed.is_empty() && empty_count * 2 > parsed.len());

            if need_fallback && !batch.is_empty() {
                log::warn!(
                    "OpenAIEnricher batch 降级单文件（解析 {} 条，空描述 {} 条）",
                    parsed.len(),
                    empty_count
                );
                // 串行单文件回退（保留原 enrich 逻辑）
                let mut out = Vec::with_capacity(batch_len);
                for (idx, req) in batch {
                    // 中断检查：收到信号后停止单文件回退（已处理的结果保留）
                    if crate::commands::enrich::is_enrich_cancelled() {
                        break;
                    }
                    let result = match self.enrich(req, categories.clone()).await {
                        Ok(r) => r,
                        Err(_) => default_enrich_result("openai"),
                    };
                    out.push((idx, result));
                }
                return out;
            }

            parsed
        })
    }

    fn name(&self) -> &str {
        "openai"
    }
}
