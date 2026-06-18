use std::sync::Arc;
use tauri::{Emitter, State};

use crate::core::config::Config;
use crate::core::models::{CatalogEntry, EnrichProgress, FileRecord};
use crate::db::catalog::CatalogDB;

use serde_json::Value;

// ────────────────── 共享状态 ──────────────────

/// AI 补全任务的运行状态。
///
/// 需要在 `lib.rs` 的 `tauri::Builder` 中通过 `.manage(SharedEnrichState::default())` 注册。
#[derive(Debug, Clone)]
pub struct EnrichState {
    pub running: bool,
    pub progress: EnrichProgress,
}

impl Default for EnrichState {
    fn default() -> Self {
        Self {
            running: false,
            progress: EnrichProgress {
                total: 0,
                done: 0,
                needs_review: 0,
                current_name: String::new(),
            },
        }
    }
}

pub type SharedEnrichState = Arc<parking_lot::Mutex<EnrichState>>;

// ────────────────── Tauri Commands ──────────────────

/// 启动异步 AI 元数据补全任务。
///
/// 使用 `parking_lot::Mutex` 防止并发双重运行。
/// 根据提供方创建 Enricher，封装为 `FallbackEnricher`（离线主 + LLM 备选），
/// 批量补全后标准化标签和分类，更新数据库。
/// 发射 `enrich_progress` / `enrich_complete` / `enrich_error` 事件。
#[tauri::command]
pub async fn start_enrich(
    app: tauri::AppHandle,
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    enrich_state: State<'_, SharedEnrichState>,
    provider: String,
    concurrency: i32,
) -> Result<(), String> {
    // ── 防止双重运行 ──
    {
        let mut state = enrich_state.lock();
        if state.running {
            return Err("AI 补全任务正在运行中，请等待完成后再试".into());
        }
        state.running = true;
        state.progress = EnrichProgress {
            total: 0,
            done: 0,
            needs_review: 0,
            current_name: String::new(),
        };
    }

    let db = db.inner().clone();
    let config = config.inner().read().clone();
    let enrich_state = enrich_state.inner().clone();

    tokio::spawn(async move {
        // 确保退出时清理状态
        defer_cleanup(&enrich_state);

        // ── 1. 获取待补全的文件 ──
        let (records, _) = match db.get_file_records("", "active", "", 1, 1_000_000) {
            Ok(r) => r,
            Err(e) => {
                let _ = app.emit("enrich_error", format!("查询文件失败: {}", e));
                return;
            }
        };

        let mut requests = Vec::new();
        let mut valid_records: Vec<FileRecord> = Vec::new();

        for r in &records {
            if r.ai_skip {
                continue;
            }
            requests.push(crate::ai::enricher::EnrichRequest {
                name: r.name.clone(),
                version: r.version.clone(),
                extension: r.extension.clone(),
                category: r.category.clone(),
                file_size: r.file_size,
                available_tags: None, // 后续从 DB 加载
            });
            valid_records.push(r.clone());
        }

        if requests.is_empty() {
            let _ = app.emit("enrich_complete", serde_json::json!({
                "total": 0,
                "done": 0,
                "needsReview": 0,
            }));
            return;
        }

        // ── 2. 获取已有标签名用于标准化 ──
        let allowed_tag_names = db.get_all_tag_names().unwrap_or_default();
        let allowed_cat_names = load_func_category_names(&config);

        for req in &mut requests {
            req.available_tags = Some(allowed_tag_names.clone());
        }

        // ── 3. 创建 Enricher ──
        let enricher = create_enricher(&provider, &config);
        let fallback = create_fallback_enricher(&provider, &config);

        // ── 4. 更新进度状态 ──
        {
            let mut state = enrich_state.lock();
            state.progress.total = requests.len();
        }
        let _ = app.emit(
            "enrich_progress",
            EnrichProgress {
                total: requests.len(),
                done: 0,
                needs_review: 0,
                current_name: String::new(),
            },
        );

        // ── 5. 批量补全（通过 channel 接收进度） ──
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::channel::<crate::ai::enricher::EnrichProgress>(16);

        let app_clone = app.clone();
        let enrich_state_clone = enrich_state.clone();
        let progress_handle = tokio::spawn(async move {
            while let Some(p) = progress_rx.recv().await {
                // 更新共享状态
                {
                    let mut state = enrich_state_clone.lock();
                    state.progress.done = p.done;
                    state.progress.current_name = p.current_name.clone();
                }
                let _ = app_clone.emit("enrich_progress", p);
            }
        });

        let effective_enricher: Box<dyn crate::ai::enricher::Enricher + Send + Sync> =
            // offline 或 custom 直接使用对应 enricher，不走 fallback
            if provider == "offline" || provider == "custom" {
                match enricher {
                    Some(e) => e,
                    None => {
                        let _ = app.emit("enrich_error", "无法创建 AI 补全器");
                        return;
                    }
                }
            } else if fallback.is_some() {
                Box::new(fallback.unwrap())
            } else {
                match enricher {
                    Some(e) => e,
                    None => {
                        let _ = app.emit("enrich_error", "无法创建 AI 补全器");
                        return;
                    }
                }
            };

        let cat_names = allowed_cat_names.clone();
        let results = crate::ai::enricher::batch_enrich(
            effective_enricher.as_ref(),
            requests,
            cat_names,
            concurrency.max(1) as usize,
            progress_tx,
        )
        .await;

        drop(progress_handle);

        // ── 6. 保存结果到数据库 ──
        let mut needs_review_count = 0usize;
        for (i, result) in results.iter().enumerate() {
            if i >= valid_records.len() {
                break;
            }

            // 标准化标签
            let normalized_tags = crate::db::catalog::normalize_tags(
                &result.tags,
                &allowed_tag_names,
            );
            let normalized_category = crate::db::catalog::normalize_functional_category(
                &result.functional_category,
                &allowed_cat_names,
            );

            // 保存为目录条目
            let entry = CatalogEntry {
                id: format!("cat_{}", &valid_records[i].file_hash[..8.min(valid_records[i].file_hash.len())]),
                name: valid_records[i].name.clone(),
                description: result.description.clone(),
                homepage_url: result.homepage_url.clone(),
                download_url: result.download_url.clone(),
                latest_version: result.latest_version.clone(),
                license: result.license.clone(),
                functional_category: normalized_category.clone(),
                tags: normalized_tags.clone(),
                ai_confidence: result.confidence,
                ai_provider: result.provider.clone(),
                meta_updated_at: chrono::Utc::now(),
                notes: String::new(),
                needs_review: result.needs_review,
                ai_skip: false,
            };

            if let Err(e) = db.insert_catalog_entry(&entry) {
                log::error!("保存目录条目失败 {}: {}", entry.name, e);
            }

            // 更新文件记录的功能分类
            if let Err(e) = db.update_file_functional_category(
                &valid_records[i].id,
                &normalized_category,
            ) {
                log::error!("更新文件功能分类失败: {}", e);
            }

            if result.needs_review {
                needs_review_count += 1;
            }
        }

        {
            let mut state = enrich_state.lock();
            state.progress.done = state.progress.total;
            state.progress.needs_review = needs_review_count;
        }

        let _ = app.emit(
            "enrich_complete",
            serde_json::json!({
                "total": results.len(),
                "done": results.len(),
                "needsReview": needs_review_count,
            }),
        );
    });

    Ok(())
}

/// 查询 AI 补全任务的运行状态和进度。
#[tauri::command]
pub async fn get_enrich_status(
    enrich_state: State<'_, SharedEnrichState>,
) -> Result<(bool, EnrichProgress), String> {
    let state = enrich_state.lock();
    Ok((state.running, state.progress.clone()))
}

// ────────────────── 辅助函数 ──────────────────

/// 根据提供方名称创建基础 Enricher。
fn create_enricher(
    provider: &str,
    config: &Config,
) -> Option<Box<dyn crate::ai::enricher::Enricher + Send + Sync>> {
    match provider {
        "openai" => Some(Box::new(
            crate::ai::openai::OpenAIEnricher::new(
                &config.ai.api_key,
                &config.ai.base_url,
            ),
        )),
        "claude" => Some(Box::new(
            crate::ai::claude::ClaudeEnricher::new(
                &config.ai.api_key,
                &config.ai.base_url,
            ),
        )),
        "ollama" => Some(Box::new(
            crate::ai::ollama::OllamaEnricher::new(&config.ai.ollama_url),
        )),
        "custom" => Some(Box::new(
            crate::ai::openai::OpenAIEnricher::new(
                &config.custom_ai_key,
                &config.custom_ai_url,
            )
            .with_model(
                if config.custom_ai_model.is_empty() {
                    "gpt-4o"
                } else {
                    &config.custom_ai_model
                },
            ),
        )),
        "offline" => {
            let db_path = config.offline_db_path();
            Some(Box::new(
                crate::ai::offline::OfflineEnricher::new(&db_path),
            ))
        }
        _ => None,
    }
}

/// 创建 FallbackEnricher：离线主 Enricher + 指定 LLM 作为备选。
fn create_fallback_enricher(
    provider: &str,
    config: &Config,
) -> Option<crate::ai::fallback::FallbackEnricher> {
    // 对于 LLM 提供方，使用 offline 作为主、LLM 作为备选
    let primary: Box<dyn crate::ai::enricher::Enricher + Send + Sync>;
    let secondary: Box<dyn crate::ai::enricher::Enricher + Send + Sync>;

    // 主 = 离线
    let offline_db_path = config.offline_db_path();
    primary = Box::new(crate::ai::offline::OfflineEnricher::new(&offline_db_path));

    // 备选 = 指定的 LLM
    secondary = match provider {
        "openai" => Box::new(crate::ai::openai::OpenAIEnricher::new(
            &config.ai.api_key,
            &config.ai.base_url,
        )),
        "claude" => Box::new(crate::ai::claude::ClaudeEnricher::new(
            &config.ai.api_key,
            &config.ai.base_url,
        )),
        "ollama" => Box::new(crate::ai::ollama::OllamaEnricher::new(&config.ai.ollama_url)),
        "custom" => Box::new(crate::ai::openai::OpenAIEnricher::new(
            &config.custom_ai_key,
            &config.custom_ai_url,
        )
        .with_model(
            if config.custom_ai_model.is_empty() {
                "gpt-4o"
            } else {
                &config.custom_ai_model
            },
        )),
        _ => return None, // 不使用 fallback
    };

    Some(crate::ai::fallback::FallbackEnricher::new(
        Some(primary),
        Some(secondary),
    ))
}

/// 从 categories.yaml 加载功能分类名称列表。
fn load_func_category_names(config: &Config) -> Vec<String> {
    let path = config.categories_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(&data) {
            if let Some(cats) = parsed.get("categories").and_then(|v| v.as_sequence()) {
                return cats
                    .iter()
                    .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect();
            }
        }
    }
    Vec::new()
}

/// RAII 守卫：在任务结束时将 enrich_state.running 设为 false。
fn defer_cleanup(state: &SharedEnrichState) {
    let state = state.clone();
    tokio::spawn(async move {
        // 等待一小段时间让主逻辑完成
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        let mut s = state.lock();
        s.running = false;
    });
}

// ────────────────── Headless Wrappers ──────────────────

pub fn start_enrich_headless(
    config: Arc<Config>,
    db: CatalogDB,
    provider: String,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<Value, String> {
    let enrich_state = SharedEnrichState::default();
    {
        let mut state = enrich_state.lock();
        if state.running {
            return Err("AI 补全任务正在运行中，请等待完成后再试".into());
        }
        state.running = true;
        state.progress = EnrichProgress {
            total: 0,
            done: 0,
            needs_review: 0,
            current_name: String::new(),
        };
    }

    let enrich_state_clone = enrich_state.clone();
    let provider_for_spawn = provider.clone();

    tokio::spawn(async move {
        defer_cleanup(&enrich_state_clone);

        // 1. 获取待补全的文件
        let (records, _) = match db.get_file_records("", "active", "", 1, 1_000_000) {
            Ok(r) => r,
            Err(e) => {
                log::error!("查询文件失败: {}", e);
                return;
            }
        };

        let mut requests = Vec::new();
        let mut valid_records: Vec<FileRecord> = Vec::new();

        for r in &records {
            if r.ai_skip {
                continue;
            }
            requests.push(crate::ai::enricher::EnrichRequest {
                name: r.name.clone(),
                version: r.version.clone(),
                extension: r.extension.clone(),
                category: r.category.clone(),
                file_size: r.file_size,
                available_tags: None,
            });
            valid_records.push(r.clone());
        }

        if requests.is_empty() {
            log::info!("无需补全的文件");
            let _ = event_tx.send(r#"{"event":"enrich_complete","data":{"total":0,"done":0,"needsReview":0}}"#.to_string());
            return;
        }

        // 2. 获取已有标签名用于标准化
        let allowed_tag_names = db.get_all_tag_names().unwrap_or_default();
        let allowed_cat_names = load_func_category_names(&config);

        for req in &mut requests {
            req.available_tags = Some(allowed_tag_names.clone());
        }

        // 3. 创建 Enricher
        let enricher = create_enricher(&provider_for_spawn, &config);
        let fallback = create_fallback_enricher(&provider_for_spawn, &config);

        // 4. 更新进度状态
        {
            let mut state = enrich_state.lock();
            state.progress.total = requests.len();
        }

        let (progress_tx, mut progress_rx) =
            tokio::sync::mpsc::channel::<crate::ai::enricher::EnrichProgress>(16);

        let enrich_state_for_progress = enrich_state.clone();
        let event_tx_for_progress = event_tx.clone();
        let progress_handle = tokio::spawn(async move {
            while let Some(p) = progress_rx.recv().await {
                {
                    let mut state = enrich_state_for_progress.lock();
                    state.progress.done = p.done;
                    state.progress.current_name = p.current_name.clone();
                }
                log::info!("enrich progress: done={}, name={}", p.done, p.current_name);
                // 广播进度事件
                let data = serde_json::to_string(&p).unwrap_or_default();
                let _ = event_tx_for_progress.send(format!("{{\"event\":\"enrich_progress\",\"data\":{}}}", data));
            }
        });

        let effective_enricher: Box<dyn crate::ai::enricher::Enricher + Send + Sync> =
            if let Some(fb) = fallback {
                Box::new(fb)
            } else {
                match enricher {
                    Some(e) => e,
                    None => {
                        log::error!("无法创建 AI 补全器");
                        return;
                    }
                }
            };

        let cat_names = allowed_cat_names.clone();
        let results = crate::ai::enricher::batch_enrich(
            effective_enricher.as_ref(),
            requests,
            cat_names,
            4, // default concurrency
            progress_tx,
        )
        .await;

        drop(progress_handle);

        // 5. 保存结果到数据库
        let mut needs_review_count = 0usize;
        for (i, result) in results.iter().enumerate() {
            if i >= valid_records.len() {
                break;
            }

            let normalized_tags =
                crate::db::catalog::normalize_tags(&result.tags, &allowed_tag_names);
            let normalized_category = crate::db::catalog::normalize_functional_category(
                &result.functional_category,
                &allowed_cat_names,
            );

            let entry = CatalogEntry {
                id: format!(
                    "cat_{}",
                    &valid_records[i].file_hash[..8.min(valid_records[i].file_hash.len())]
                ),
                name: valid_records[i].name.clone(),
                description: result.description.clone(),
                homepage_url: result.homepage_url.clone(),
                download_url: result.download_url.clone(),
                latest_version: result.latest_version.clone(),
                license: result.license.clone(),
                functional_category: normalized_category.clone(),
                tags: normalized_tags.clone(),
                ai_confidence: result.confidence,
                ai_provider: result.provider.clone(),
                meta_updated_at: chrono::Utc::now(),
                notes: String::new(),
                needs_review: result.needs_review,
                ai_skip: false,
            };

            if let Err(e) = db.insert_catalog_entry(&entry) {
                log::error!("保存目录条目失败 {}: {}", entry.name, e);
            }

            if let Err(e) = db.update_file_functional_category(
                &valid_records[i].id,
                &normalized_category,
            ) {
                log::error!("更新文件功能分类失败: {}", e);
            }

            if result.needs_review {
                needs_review_count += 1;
            }
        }

        {
            let mut state = enrich_state.lock();
            state.progress.done = state.progress.total;
            state.progress.needs_review = needs_review_count;
        }

        log::info!(
            "AI 补全完成: total={}, needs_review={}",
            results.len(),
            needs_review_count
        );
        // 广播完成事件
        let _ = event_tx.send(format!("{{\"event\":\"enrich_complete\",\"data\":{{\"total\":{},\"done\":{},\"needsReview\":{}}}}}", results.len(), results.len(), needs_review_count));
    });

    Ok(serde_json::json!({"started": true, "provider": provider}))
}
