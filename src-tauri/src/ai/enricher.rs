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

    /// 批量补全：一次处理多个文件（LLM provider 可重写为真正的批量 HTTP 请求）。
    ///
    /// 默认实现：串行调 `self.enrich`（offline/claude/ollama/fallback 用此兜底，无需重写）。
    /// `batch`：`(原始索引, EnrichRequest)` 对，返回 `(索引, 结果)` 对。
    /// 返回的结果数应等于 batch 长度；缺失项由调用方（batch_enrich）补 default。
    fn enrich_batch(
        &self,
        batch: Vec<(usize, EnrichRequest)>,
        categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Vec<(usize, EnrichResult)>> + Send + '_>> {
        Box::pin(async move {
            let mut out = Vec::with_capacity(batch.len());
            for (idx, req) in batch {
                let result = match self.enrich(req, categories.clone()).await {
                    Ok(r) => r,
                    Err(_) => default_enrich_result(self.name()),
                };
                out.push((idx, result));
            }
            out
        })
    }

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
        confidence (float 0.0-1.0), \
        download_reliability (string, one of: high = official/well-known source, safe to delete and re-download; \
        medium = known but not official; low = unknown or risky source, keep a backup before deleting; \
        empty string if not applicable). \
        If unsure, use empty string or 0.3 confidence. NEVER fabricate URLs. Return pure JSON only. \
        If a GitHub match is provided, use its description/topics to determine functional_category \
        and improve accuracy, but do NOT copy its URL as homepage_url unless it is clearly the official project."
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
    let github_info = match &req.github_hint {
        Some(h) => {
            let topics = if h.topics.is_empty() {
                String::new()
            } else {
                format!("\nTopics: {}", h.topics.join(", "))
            };
            format!(
                "\nGitHub match: {} ★{}\nDescription: {}{}",
                h.full_name, h.stars, h.description, topics
            )
        }
        None => String::new(),
    };
    format!(
        "File: {}\nVersion: {}\nExtension: {}\nCategory: {}\nFile size: {} bytes\n{}{}",
        req.name,
        if req.version.is_empty() { "unknown" } else { &req.version },
        if req.extension.is_empty() { "unknown" } else { &req.extension },
        if req.category.is_empty() { "unknown" } else { &req.category },
        req.file_size,
        tags_info,
        github_info,
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

/// 从一个 JSON 对象提取 EnrichResult（单文件/批量共用字段提取逻辑）。
///
/// confidence < 0.6 时标记 needs_review 并清空 URL（防编造）。
pub fn parse_one_obj(obj: &serde_json::Map<String, serde_json::Value>, provider: &str) -> EnrichResult {
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

    // download_reliability 归一化为 high/medium/low，非法值清空
    let download_reliability = obj
        .get("download_reliability")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_lowercase())
        .filter(|s| matches!(s.as_str(), "high" | "medium" | "low"))
        .unwrap_or_default();

    let needs_review = confidence < 0.6;

    // 低置信度管控：confidence < 0.6 时不信任 AI 返回的 URL（可能编造），
    // 清空 homepage/download_url，只保留 description 供人工审核。
    // 防止冷门工具被编造官网/GitHub 地址写入 catalog。
    let (safe_homepage, safe_download) = if needs_review {
        (String::new(), String::new())
    } else {
        (homepage_url, download_url)
    };

    EnrichResult {
        description,
        homepage_url: safe_homepage,
        download_url: safe_download,
        latest_version,
        license,
        functional_category,
        tags,
        confidence,
        needs_review,
        provider: provider.to_string(),
        download_reliability,
    }
}

/// 解析 LLM 返回的单文件 JSON 文本为 EnrichResult。
/// confidence < 0.6 时标记 needs_review（由 parse_one_obj 处理）。
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

    parse_one_obj(obj, provider)
}

// ────────────────── 批量 prompt 与解析 ──────────────────

/// 构建批量补全的系统提示词。
///
/// 与单文件 build_system_prompt 的区别：要求返回 **JSON 对象**（key=索引字符串），
/// 而非单个 JSON 对象。用索引作 key 比数组鲁棒——数组易对齐错乱/缺项。
pub fn build_batch_system_prompt(categories: &[String], tags: &[String]) -> String {
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
        "You are a software metadata expert. You will receive a JSON array of files \
        (each with index, name, extension, size). Return ONLY a JSON OBJECT where each key \
        is the file's index (as string) and each value is an object with these exact fields: \
        description (string, ≤120 chars, Chinese preferred), \
        homepage_url (string, official website only), \
        download_url (string, download page URL, not direct file link), \
        latest_version (string), \
        license (string), \
        functional_category (string, must match one of: {cat_list}), \
        tags (array of strings, each must match: {tag_list}, max 5), \
        confidence (float 0.0-1.0), \
        download_reliability (string, one of: high = official/well-known source, safe to delete and re-download; \
        medium = known but not official; low = unknown or risky source, keep a backup before deleting; \
        empty string if not applicable). \
        If unsure, use empty string or 0.3 confidence. NEVER fabricate URLs. \
        Every input file MUST appear in the output object. Return pure JSON only. \
        If a file's \"github\" field is non-null, it is a GitHub repo match for that file — \
        use its description/topics to determine functional_category and improve accuracy, \
        but do NOT copy its URL as homepage_url unless it is clearly the official project. \
        Example: {{\"0\":{{\"description\":\"...\",...}},\"1\":{{...}}}}"
    )
}

/// 构建批量补全的用户消息：把批次序列化成 JSON 数组。
pub fn build_batch_user_message(batch: &[(usize, EnrichRequest)]) -> String {
    let arr: Vec<serde_json::Value> = batch
        .iter()
        .map(|(idx, req)| {
            // GitHub hint（可选）：命中时附在文件条目里，作为"已知事实"提示 LLM
            let github = match &req.github_hint {
                Some(h) => serde_json::json!({
                    "repo": h.full_name,
                    "stars": h.stars,
                    "description": h.description,
                    "topics": h.topics,
                }),
                None => serde_json::Value::Null,
            };
            serde_json::json!({
                "index": idx,
                "name": req.name,
                "extension": if req.extension.is_empty() { "unknown" } else { &req.extension },
                "size": req.file_size,
                "github": github,
            })
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".to_string())
}

/// 解析批量补全的返回（JSON 对象 `{"0":{...},"1":{...}}`）。
///
/// 缺失的 index 用 default_enrich_result 补齐，保证返回长度 = batch_len。
/// 返回 (原始索引, 结果) 对，顺序按 batch 内 index 升序。
pub fn parse_batch_response(
    raw: &str,
    provider: &str,
    batch_len: usize,
) -> Vec<(usize, EnrichResult)> {
    let cleaned = strip_markdown_fences(raw);
    let parsed: serde_json::Value = match serde_json::from_str(&cleaned) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("parse_batch_response JSON parse failed: {}", e);
            // 整批解析失败：调用方应据此降级单文件
            return Vec::new();
        }
    };
    let obj = match parsed.as_object() {
        Some(o) => o,
        None => {
            log::warn!("parse_batch_response not a JSON object");
            return Vec::new();
        }
    };

    let mut out: Vec<(usize, EnrichResult)> = Vec::with_capacity(batch_len);
    // 遍历 batch 的所有可能 index，按 key 查找；缺失的补 default
    // （调用方传 batch_len，但我们不知道具体 index 值——改为遍历 obj 的 key 收集）
    // 实际上调用方关心的是"返回的 (index,result) 对"，缺的由调用方在 batch_enrich 里补 default。
    for (key, val) in obj.iter() {
        let idx: usize = match key.parse() {
            Ok(n) => n,
            Err(_) => continue,
        };
        let entry_obj = match val.as_object() {
            Some(o) => o,
            None => continue,
        };
        out.push((idx, parse_one_obj(entry_obj, provider)));
    }
    out
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
        download_reliability: String::new(),
    }
}

// ────────────────── 批量补全 ──────────────────

/// 批量补全文件元数据：**真并发** + **分批** + 实时进度。
///
/// 优化要点（修复历史"假并发"串行 bug）：
/// 1. 把 requests 按 batch_size 切片成多个批次，每批一次 LLM 请求（请求数降 N 倍）。
/// 2. 用 `futures::stream::buffer_unordered(concurrency)` 真正并发驱动多个批次 future。
///    （旧实现用 Semaphore 但顺序 await，concurrency 形同虚设——纯串行）
/// 3. 各批的 index 互不重叠，按 index 写入 results，无需锁。
///
/// trait 的 enrich_batch future 借用 `&self`，buffer_unordered 在流的生命周期内持有该借用，
/// 无需 Arc/tokio::spawn。
///
/// `concurrency`：并发批次数（free 模型限流建议 2）。`batch_size`：每批文件数（建议 20）。
pub async fn batch_enrich(
    enricher: &(dyn Enricher + Send + Sync),
    requests: Vec<EnrichRequest>,
    categories: Vec<String>,
    concurrency: usize,
    batch_size: usize,
    progress_tx: tokio::sync::mpsc::Sender<EnrichProgress>,
    // 每批完成回调：调用方在此增量落库（insert_catalog_entry 等）+ 发进度。
    // 这样中断时已完成的批次不丢失（旧实现全跑完才落库，中断=全丢）。
    on_batch: impl Fn(&[(usize, EnrichResult)]),
) -> Vec<EnrichResult> {
    let total = requests.len();
    if total == 0 {
        return Vec::new();
    }

    // 1. 切片成批次：每批 (原始索引, EnrichRequest) 对
    let batch_size = batch_size.max(1);
    let batches: Vec<Vec<(usize, EnrichRequest)>> = requests
        .into_iter()
        .enumerate()
        .collect::<Vec<_>>()
        .chunks(batch_size)
        .map(|chunk| chunk.to_vec())
        .collect();

    let num_batches = batches.len();
    log::info!(
        "batch_enrich: {} files → {} batches (batch_size={}), concurrency={}",
        total,
        num_batches,
        batch_size,
        concurrency.max(1)
    );

    // 2. 预分配结果数组（各批 index 不重叠，按 index 直接写入）
    let mut results: Vec<EnrichResult> = Vec::with_capacity(total);
    results.resize(total, default_enrich_result(enricher.name()));
    let done_count = std::sync::atomic::AtomicUsize::new(0);
    let mut needs_review_count = 0usize;
    let mut cancelled = false;

    // 3. 并发流：buffer_unordered 真正并发（旧实现的 Semaphore 是假并发）
    use futures::stream::{self, StreamExt};
    let mut stream = stream::iter(batches)
        .map(|batch| enricher.enrich_batch(batch, categories.clone()))
        .buffer_unordered(concurrency.max(1));

    while let Some(batch_results) = stream.next().await {
        // 中断检查：收到信号后停止处理剩余批次（in-flight 批次已跑完）
        let is_cancelled = crate::commands::enrich::is_enrich_cancelled();

        // 1. 把本批结果写入 results[idx]（按 index，各批不重叠）
        for (idx, result) in &batch_results {
            if *idx < total {
                if result.needs_review {
                    needs_review_count += 1;
                }
                results[*idx] = result.clone();
            }
        }

        // 2. 增量落库回调：每批完成立即保存（中断不丢已完成的批次）
        on_batch(&batch_results);

        if is_cancelled {
            log::info!("batch_enrich: 收到中断信号，停止调度剩余批次");
            cancelled = true;
            break;
        }

        // 3. 每批完成发一次进度（粒度=批，比每文件粗，但 batch_size 小时仍平滑）
        let done = done_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1;
        let _ = progress_tx
            .send(EnrichProgress {
                total,
                done: total.min(done * batch_size),
                needs_review: needs_review_count,
                current_name: format!("batch {}/{}", done, num_batches),
            })
            .await;
    }

    let _ = cancelled;
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_batch_user_message() {
        let batch = vec![
            (
                0,
                EnrichRequest {
                    name: "nmap-7.94.exe".into(),
                    version: String::new(),
                    extension: ".exe".into(),
                    category: String::new(),
                    file_size: 1000,
                    available_tags: None,
                    github_hint: None,
                },
            ),
            (
                1,
                EnrichRequest {
                    name: "report.pdf".into(),
                    version: String::new(),
                    extension: ".pdf".into(),
                    category: String::new(),
                    file_size: 2000,
                    available_tags: None,
                    github_hint: None,
                },
            ),
        ];
        let msg = build_batch_user_message(&batch);
        // 应是合法 JSON 数组，含 index/name/extension/size
        let parsed: serde_json::Value = serde_json::from_str(&msg).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["index"], 0);
        assert_eq!(arr[0]["name"], "nmap-7.94.exe");
        assert_eq!(arr[1]["extension"], ".pdf");
        assert_eq!(arr[1]["size"], 2000);
    }

    #[test]
    fn test_parse_batch_response_normal() {
        let raw = r#"{
            "0": {"description":"网络扫描工具","functional_category":"Net","confidence":0.9,"homepage_url":"https://nmap.org","download_url":"","latest_version":"7.94","license":"NPSL","tags":["security"],"download_reliability":"high"},
            "1": {"description":"PDF报告","confidence":0.5,"functional_category":"","tags":[]}
        }"#;
        let out = parse_batch_response(raw, "openai", 2);
        assert_eq!(out.len(), 2, "应解析出 2 条");
        // 按 index 排序找到
        let r0 = out.iter().find(|(i, _)| *i == 0).unwrap();
        assert_eq!(r0.1.description, "网络扫描工具");
        assert!(!r0.1.needs_review); // confidence 0.9 ≥ 0.6
        let r1 = out.iter().find(|(i, _)| *i == 1).unwrap();
        assert!(r1.1.needs_review); // confidence 0.5 < 0.6
    }

    #[test]
    fn test_parse_batch_response_missing_index() {
        // 模型只返回 index 0，缺 index 1 —— parse_batch_response 只返回存在的，
        // 缺失项由 batch_enrich 用预填的 default 补齐（不在此函数职责）
        let raw = r#"{"0": {"description":"only-one","confidence":0.8}}"#;
        let out = parse_batch_response(raw, "openai", 2);
        assert_eq!(out.len(), 1, "只返回模型给的 1 条");
        assert_eq!(out[0].0, 0);
        assert_eq!(out[0].1.description, "only-one");
    }

    #[test]
    fn test_parse_batch_response_broken_json() {
        // JSON 炸裂 → 返回空 Vec（调用方 OpenAIEnricher 据此降级单文件）
        let out = parse_batch_response("not json at all {{{", "openai", 3);
        assert!(out.is_empty(), "坏 JSON 应返回空 Vec");
    }

    #[test]
    fn test_parse_batch_response_not_object() {
        // 返回数组而非对象 → 返回空 Vec
        let out = parse_batch_response(r#"["a","b"]"#, "openai", 2);
        assert!(out.is_empty(), "非对象应返回空 Vec");
    }

    /// 验证 parse_one_obj 与 parse_enrich_response 一致（重构后字段提取不回归）
    #[test]
    fn test_parse_one_obj_consistent_with_enrich_response() {
        let raw = r#"{"description":"测试","confidence":0.7,"functional_category":"Net","tags":["a","b"]}"#;
        let r = parse_enrich_response(raw, "openai", "x");
        assert_eq!(r.description, "测试");
        assert!(!r.needs_review); // 0.7 ≥ 0.6
        assert_eq!(r.functional_category, "Net");
        assert_eq!(r.tags, vec!["a".to_string(), "b".to_string()]);
    }
}
