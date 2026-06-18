use std::future::Future;
use std::pin::Pin;

// ────────────────── 类型重导出 ──────────────────

pub use crate::core::models::{EnrichProgress, EnrichRequest, EnrichResult};

// ────────────────── Enricher Trait ──────────────────

/// AI 元数据补全的统一 trait。
///
/// 所有 AI 提供方（在线/离线）均实现此 trait，供上层统一调用和 fallback 链使用。
/// 使用手动 boxed future 代替 async_trait 以避免额外依赖。
pub trait Enricher: Send + Sync {
    fn enrich(
        &self,
        req: EnrichRequest,
        categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<EnrichResult, Box<dyn std::error::Error + Send + Sync>>> + Send + '_>>;

    fn name(&self) -> &str;
}

// ────────────────── 公共辅助函数 ──────────────────

/// 构建系统提示词，嵌入可选分类和标签约束。
pub fn build_system_prompt(categories: &[String], tags: &[String]) -> String {
    let cat_list = if categories.is_empty() {
        "general".to_string()
    } else {
        categories.join(", ")
    };
    let tag_list = if tags.is_empty() {
        "any".to_string()
    } else {
        tags.join(", ")
    };
    format!(
        "You are a software metadata expert. Given a file name, version, and category, \
        return ONLY a JSON object with these exact fields: \
        description (string, ≤120 chars, Chinese preferred), \
        homepage_url (string, official website only), \
        download_url (string, download page URL, not direct file link), \
        latest_version (string), \
        license (string), \
        functional_category (string, must match one of: {cat_list}), \
        tags (array of strings, each must match: {tag_list}, max 5), \
        confidence (float 0.0-1.0). \
        If unsure, use empty string or 0.3 confidence. NEVER fabricate URLs. Return pure JSON only."
    )
}

/// 构建用户消息，包含文件基本信息。
pub fn build_user_message(req: &EnrichRequest) -> String {
    let tags_info = match &req.available_tags {
        Some(tags) if !tags.is_empty() => {
            format!("Available tags: {}", tags.join(", "))
        }
        _ => String::new(),
    };
    format!(
        "File: {}\nVersion: {}\nExtension: {}\nCategory: {}\nFile size: {} bytes\n{}",
        req.name,
        if req.version.is_empty() { "unknown" } else { &req.version },
        if req.extension.is_empty() { "unknown" } else { &req.extension },
        if req.category.is_empty() { "unknown" } else { &req.category },
        req.file_size,
        tags_info,
    )
}

/// 清理 LLM 返回的 markdown 代码围栏。
pub fn strip_markdown_fences(s: &str) -> String {
    let s = s.trim();
    // 移除开头的 ```json / ```
    let s = s.trim_start_matches("```json");
    let s = s.trim_start_matches("```");
    // 移除结尾的 ```
    let s = s.trim_end_matches("```");
    s.trim().to_string()
}

/// 解析 LLM 返回的 JSON 文本为 EnrichResult。
/// confidence < 0.6 时标记 needs_review。
pub fn parse_enrich_response(raw: &str, provider: &str, name: &str) -> EnrichResult {
    let cleaned = strip_markdown_fences(raw);

    let parsed: serde_json::Value = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("parse_enrich_response '{}' JSON parse failed: {}", name, e);
            return default_enrich_result(provider);
        }
    };

    let obj = match parsed.as_object() {
        Some(o) => o,
        None => {
            log::warn!("parse_enrich_response '{}' not a JSON object", name);
            return default_enrich_result(provider);
        }
    };

    let description = obj
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let homepage_url = obj
        .get("homepage_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let download_url = obj
        .get("download_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let latest_version = obj
        .get("latest_version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let license = obj
        .get("license")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let functional_category = obj
        .get("functional_category")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tags = obj
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let confidence = obj
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.3);

    let needs_review = confidence < 0.6;

    EnrichResult {
        description,
        homepage_url,
        download_url,
        latest_version,
        license,
        functional_category,
        tags,
        confidence,
        needs_review,
        provider: provider.to_string(),
    }
}

/// 创建默认的（空的）补全结果，标记需要人工审核。
pub fn default_enrich_result(provider: &str) -> EnrichResult {
    EnrichResult {
        description: String::new(),
        homepage_url: String::new(),
        download_url: String::new(),
        latest_version: String::new(),
        license: String::new(),
        functional_category: String::new(),
        tags: Vec::new(),
        confidence: 0.0,
        needs_review: true,
        provider: provider.to_string(),
    }
}

// ────────────────── 批量补全 ──────────────────

/// 批量补全文件元数据，通过 Semaphore 控制并发上限，通过 mpsc channel 实时汇报进度。
///
/// 由于 trait 的 enrich future 借用了 `&self`，无法通过 tokio::spawn 实现 true concurrency，
/// 因此采用串行处理 + Semaphore 限流的策略，每次仅持有一个 permit 后发起 HTTP 请求并 await。
pub async fn batch_enrich(
    enricher: &(dyn Enricher + Send + Sync),
    requests: Vec<EnrichRequest>,
    categories: Vec<String>,
    concurrency: usize,
    progress_tx: tokio::sync::mpsc::Sender<EnrichProgress>,
) -> Vec<EnrichResult> {
    let total = requests.len();
    if total == 0 {
        return Vec::new();
    }

    let sem = tokio::sync::Semaphore::new(concurrency.max(1));
    let mut results: Vec<EnrichResult> = Vec::with_capacity(total);
    let mut needs_review_count = 0usize;

    for (_i, req) in requests.into_iter().enumerate() {
        let name = req.name.clone();

        // 获取 Semaphore permit 控制并发
        let _permit = sem.acquire().await.unwrap();

        let result = match enricher.enrich(req, categories.clone()).await {
            Ok(r) => r,
            Err(e) => {
                log::warn!("enrich '{}' failed: {}", name, e);
                default_enrich_result(enricher.name())
            }
        };
        drop(_permit);

        if result.needs_review {
            needs_review_count += 1;
        }

        // 发送进度
        let _ = progress_tx
            .send(EnrichProgress {
                total,
                done: results.len() + 1,
                needs_review: needs_review_count,
                current_name: name,
            })
            .await;

        results.push(result);
    }

    results
}
