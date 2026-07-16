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

// ────────────────── 中断标志（镜像 scan 的 AtomicBool 模式）────────────

/// AI 补全取消标志（模块级，enrich:cancel action 设置为 true）。
///
/// batch_enrich 流循环 + enrich_batch 单文件降级循环会检查此标志：
/// - 收到中断信号后，不再调度新批次（in-flight 批次 ≤concurrency 个会跑完）。
/// - 已完成的批次通过 on_batch 回调已立即落库，中断 0 丢失。
static ENRICH_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 请求取消当前 AI 补全任务
pub fn request_enrich_cancel() {
    ENRICH_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// 检查 AI 补全是否被取消（供 batch_enrich / enrich_batch 调用）
pub fn is_enrich_cancelled() -> bool {
    ENRICH_CANCEL.load(std::sync::atomic::Ordering::SeqCst)
}

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
        // 重置中断标志（新一轮补全开始）
        ENRICH_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
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
                github_hint: None,    // 后续 GitHub 搜索阶段填充
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
        let allowed_cat_names = load_func_category_names(&db);

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
        // 增量落库（与 headless 路径同构，中断不丢已完成批次）
        let db_for_save = db.clone();
        let state_for_save = enrich_state.clone();
        let app_for_save = app.clone();
        let records_ref = valid_records.clone();
        let allowed_tags_clone = allowed_tag_names.clone();
        let allowed_cats_clone = allowed_cat_names.clone();
        let nr_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let saved_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let nr_clone = nr_counter.clone();
        let saved_clone = saved_counter.clone();

        let results = crate::ai::enricher::batch_enrich(
            effective_enricher.as_ref(),
            requests,
            cat_names,
            concurrency.max(1) as usize,
            (config.ai_batch_size as usize).max(1),
            progress_tx,
            move |batch_results: &[(usize, crate::ai::enricher::EnrichResult)]| {
                for &(idx, ref result) in batch_results {
                    if idx >= records_ref.len() {
                        continue;
                    }
                    let rec = &records_ref[idx];
                    let normalized_tags =
                        crate::db::catalog::normalize_tags(&result.tags, &allowed_tags_clone);
                    let normalized_category = crate::db::catalog::normalize_functional_category(
                        &result.functional_category,
                        &allowed_cats_clone,
                    );
                    let entry = CatalogEntry {
                        id: format!("cat_{}", &rec.file_hash[..8.min(rec.file_hash.len())]),
                        name: rec.name.clone(),
                        description: result.description.clone(),
                        homepage_url: result.homepage_url.clone(),
                        download_url: result.download_url.clone(),
                        latest_version: result.latest_version.clone(),
                        license: result.license.clone(),
                        functional_category: normalized_category.clone(),
                        tags: normalized_tags,
                        ai_confidence: result.confidence,
                        ai_provider: result.provider.clone(),
                        meta_updated_at: chrono::Utc::now(),
                        notes: String::new(),
                        needs_review: result.needs_review,
                        ai_skip: false,
                        download_reliability: result.download_reliability.clone(),
                    };
                    if let Err(e) = db_for_save.insert_catalog_entry(&entry) {
                        log::error!("保存目录条目失败 {}: {}", entry.name, e);
                    }
                    if let Err(e) =
                        db_for_save.update_file_functional_category(&rec.id, &normalized_category)
                    {
                        log::error!("更新文件功能分类失败: {}", e);
                    }
                    saved_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if result.needs_review {
                        nr_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                }
                let saved = saved_clone.load(std::sync::atomic::Ordering::Relaxed);
                let nr = nr_clone.load(std::sync::atomic::Ordering::Relaxed);
                {
                    let mut state = state_for_save.lock();
                    state.progress.done = saved;
                    state.progress.needs_review = nr;
                }
                let _ = app_for_save.emit(
                    "enrich_progress",
                    serde_json::json!({ "saved": saved, "needsReview": nr }),
                );
            },
        )
        .await;

        drop(progress_handle);

        let needs_review_count = nr_counter.load(std::sync::atomic::Ordering::Relaxed);
        let saved = saved_counter.load(std::sync::atomic::Ordering::Relaxed);
        let was_cancelled = is_enrich_cancelled();

        {
            let mut state = enrich_state.lock();
            state.progress.done = saved;
            state.progress.needs_review = needs_review_count;
        }

        if was_cancelled {
            let _ = app.emit(
                "enrich_cancelled",
                serde_json::json!({
                    "saved": saved,
                    "needsReview": needs_review_count,
                    "total": results.len(),
                }),
            );
        } else {
            let _ = app.emit(
                "enrich_complete",
                serde_json::json!({
                    "total": results.len(),
                    "done": saved,
                    "needsReview": needs_review_count,
                }),
            );
        }
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

// ────────────────── Headless Wrappers ──────────────────

/// headless 版状态查询：不依赖 Tauri State。
pub fn get_enrich_status_headless(enrich_state: &SharedEnrichState) -> Value {
    let state = enrich_state.lock();
    serde_json::json!({
        "running": state.running,
        "total": state.progress.total,
        "done": state.progress.done,
        "needs_review": state.progress.needs_review,
        "current_name": state.progress.current_name,
    })
}

/// headless 版 AI 补全：不依赖 Tauri State/AppHandle，通过 event_tx 广播事件。
///
/// 事件 JSON：`{"event":"enrich_progress|enrich_complete|enrich_error","data":...}`
pub async fn start_enrich_headless(
    db: Arc<CatalogDB>,
    config: Config,
    enrich_state: SharedEnrichState,
    provider: String,
    concurrency: i32,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), String> {
    // ── 防止双重运行 ──
    {
        let mut state = enrich_state.lock();
        if state.running {
            return Err("AI 补全任务正在运行中，请等待完成后再试".into());
        }
        state.running = true;
        // 重置中断标志（新一轮补全开始）
        ENRICH_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
        state.progress = EnrichProgress {
            total: 0,
            done: 0,
            needs_review: 0,
            current_name: String::new(),
        };
    }

    let emit_event = |tx: &tokio::sync::broadcast::Sender<String>, event: &str, data: Value| {
        let _ = tx.send(serde_json::json!({ "event": event, "data": data }).to_string());
    };

    defer_cleanup(&enrich_state);

    // ── 1. 获取待补全的文件 ──
    let (records, _) = match db.get_file_records("", "active", "", 1, 1_000_000) {
        Ok(r) => r,
        Err(e) => {
            emit_event(&event_tx, "enrich_error", Value::String(format!("查询文件失败: {}", e)));
            return Err(e.to_string());
        }
    };

    // 续传：查已有 catalog_entries 的 name 集合，跳过已成功丰富（ai_provider 非空）的文件。
    // 中断后重启自动从断点继续，无需单独的进度表（catalog_entries.name 是 UNIQUE）。
    let existing_names: std::collections::HashSet<String> = db
        .get_catalog_entries("", 1, 100_000)
        .unwrap_or_default()
        .0
        .into_iter()
        .filter(|c| !c.ai_provider.is_empty())
        .map(|c| c.name.to_lowercase())
        .collect();

    let mut requests = Vec::new();
    let mut valid_records: Vec<FileRecord> = Vec::new();
    let mut skipped = 0usize;

    for r in &records {
        if r.ai_skip {
            continue;
        }
        if existing_names.contains(&r.name.to_lowercase()) {
            skipped += 1;
            continue;
        }
        requests.push(crate::ai::enricher::EnrichRequest {
            name: r.name.clone(),
            version: r.version.clone(),
            extension: r.extension.clone(),
            category: r.category.clone(),
            file_size: r.file_size,
            available_tags: None,
            github_hint: None,
        });
        valid_records.push(r.clone());
    }
    if skipped > 0 {
        log::info!("enrich: 跳过 {} 个已丰富文件，{} 个待处理", skipped, requests.len());
    }

    if requests.is_empty() {
        emit_event(&event_tx, "enrich_complete", serde_json::json!({
            "total": 0, "done": 0, "needsReview": 0,
        }));
        return Ok(());
    }

    // ── 1.5 GitHub 搜索增强：为每个文件搜 GitHub，命中则填 github_hint ──
    // 思路：文件多是 GitHub 下载的原始名，搜 GitHub 拿到仓库事实（描述/topics），
    // 塞进 enrich prompt 作为"已知事实"，提升功能分类准确性。
    // 受 GitHub 限流（认证 30/min、未认证 10/min），串行搜索 + 中断检查。
    if config.enable_github_search {
        let token = if config.github_token.is_empty() {
            None
        } else {
            Some(config.github_token.clone())
        };
        let is_authed = token.is_some();
        let total_reqs = requests.len();
        let searcher = crate::ai::github_search::GitHubSearcher::new(token);
        let mut hint_count = 0usize;
        log::info!(
            "enrich: GitHub 搜索增强已启用（{}），开始搜索 {} 个文件",
            if is_authed { "已认证 30/min" } else { "未认证 10/min" },
            total_reqs
        );
        for (i, req) in requests.iter_mut().enumerate() {
            // 中断检查
            if is_enrich_cancelled() {
                log::info!("enrich: GitHub 搜索阶段被中断（已搜 {}，命中 {}）", i, hint_count);
                break;
            }
            if let Some(hint) = searcher.find_best_match(&req.name).await {
                req.github_hint = Some(hint);
                hint_count += 1;
            }
            // 进度汇报（搜索阶段）
            let _ = event_tx.send(
                serde_json::json!({
                    "event": "enrich_progress",
                    "data": {
                        "stage": "github_search",
                        "done": i + 1,
                        "total": total_reqs,
                        "hits": hint_count,
                    }
                })
                .to_string(),
            );
        }
        log::info!("enrich: GitHub 搜索完成，{}/{} 命中", hint_count, requests.len());
    }

    // ── 2. 获取已有标签名 ──
    let allowed_tag_names = db.get_all_tag_names().unwrap_or_default();
    let allowed_cat_names = load_func_category_names(&db);

    for req in &mut requests {
        req.available_tags = Some(allowed_tag_names.clone());
    }

    // ── 3. 创建 Enricher ──
    let enricher = create_enricher(&provider, &config);
    let fallback = create_fallback_enricher(&provider, &config);

    // ── 4. 更新进度 ──
    {
        let mut state = enrich_state.lock();
        state.progress.total = requests.len();
    }
    emit_event(
        &event_tx,
        "enrich_progress",
        serde_json::to_value(EnrichProgress {
            total: requests.len(),
            done: 0,
            needs_review: 0,
            current_name: String::new(),
        })
        .unwrap_or_default(),
    );

    // ── 5. 批量补全 ──
    let (progress_tx, mut progress_rx) =
        tokio::sync::mpsc::channel::<crate::ai::enricher::EnrichProgress>(16);

    let enrich_state_clone = enrich_state.clone();
    let event_tx_clone = event_tx.clone();
    let progress_handle = tokio::spawn(async move {
        while let Some(p) = progress_rx.recv().await {
            {
                let mut state = enrich_state_clone.lock();
                state.progress.done = p.done;
                state.progress.current_name = p.current_name.clone();
            }
            let _ = event_tx_clone.send(
                serde_json::json!({ "event": "enrich_progress", "data": p }).to_string(),
            );
        }
    });

    let effective_enricher: Box<dyn crate::ai::enricher::Enricher + Send + Sync> =
        if provider == "offline" || provider == "custom" {
            match enricher {
                Some(e) => e,
                None => {
                    emit_event(&event_tx, "enrich_error", Value::String("无法创建 AI 补全器".into()));
                    drop(progress_handle);
                    return Err("无法创建 AI 补全器".into());
                }
            }
        } else if fallback.is_some() {
            Box::new(fallback.unwrap())
        } else {
            match enricher {
                Some(e) => e,
                None => {
                    emit_event(&event_tx, "enrich_error", Value::String("无法创建 AI 补全器".into()));
                    drop(progress_handle);
                    return Err("无法创建 AI 补全器".into());
                }
            }
        };

    let cat_names = allowed_cat_names.clone();
    // ── 增量落库：on_batch 回调把保存逻辑移到"每批完成立即落库"（中断不丢）──
    // 闭包捕获：valid_records 按引用、db/enrich_state/event_tx 各 clone。
    let db_for_save = db.clone();
    let state_for_save = enrich_state.clone();
    let tx_for_save = event_tx.clone();
    let allowed_tags_save = allowed_cat_names.clone(); // 占位避免警告，实际用 allowed_tag_names
    let _ = allowed_tags_save;
    let needs_review_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let saved_counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let records_ref = valid_records.clone(); // 闭包需 'static 语义，clone 一份（FileRecord 较轻）
    let allowed_tags_clone = allowed_tag_names.clone();
    let allowed_cats_clone = allowed_cat_names.clone();
    let nr_counter_clone = needs_review_counter.clone();
    let saved_counter_clone = saved_counter.clone();

    let results = crate::ai::enricher::batch_enrich(
        effective_enricher.as_ref(),
        requests,
        cat_names,
        concurrency.max(1) as usize,
        (config.ai_batch_size as usize).max(1),
        progress_tx,
        move |batch_results: &[(usize, crate::ai::enricher::EnrichResult)]| {
            // 每批完成立即落库：normalize + insert_catalog_entry + update_file_functional_category
            for &(idx, ref result) in batch_results {
                if idx >= records_ref.len() {
                    continue;
                }
                let rec = &records_ref[idx];

                let normalized_tags =
                    crate::db::catalog::normalize_tags(&result.tags, &allowed_tags_clone);
                let normalized_category = crate::db::catalog::normalize_functional_category(
                    &result.functional_category,
                    &allowed_cats_clone,
                );

                let entry = CatalogEntry {
                    id: format!(
                        "cat_{}",
                        &rec.file_hash[..8.min(rec.file_hash.len())]
                    ),
                    name: rec.name.clone(),
                    description: result.description.clone(),
                    homepage_url: result.homepage_url.clone(),
                    download_url: result.download_url.clone(),
                    latest_version: result.latest_version.clone(),
                    license: result.license.clone(),
                    functional_category: normalized_category.clone(),
                    tags: normalized_tags,
                    ai_confidence: result.confidence,
                    ai_provider: result.provider.clone(),
                    meta_updated_at: chrono::Utc::now(),
                    notes: String::new(),
                    needs_review: result.needs_review,
                    ai_skip: false,
                    download_reliability: result.download_reliability.clone(),
                };

                if let Err(e) = db_for_save.insert_catalog_entry(&entry) {
                    log::error!("保存目录条目失败 {}: {}", entry.name, e);
                }
                if let Err(e) =
                    db_for_save.update_file_functional_category(&rec.id, &normalized_category)
                {
                    log::error!("更新文件功能分类失败: {}", e);
                }

                saved_counter_clone
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if result.needs_review {
                    nr_counter_clone
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
            // 更新 enrich_state.progress（已保存数）
            let saved = saved_counter_clone.load(std::sync::atomic::Ordering::Relaxed);
            let nr = nr_counter_clone.load(std::sync::atomic::Ordering::Relaxed);
            {
                let mut state = state_for_save.lock();
                state.progress.done = saved;
                state.progress.needs_review = nr;
            }
            let _ = tx_for_save.send(
                serde_json::json!({
                    "event": "enrich_progress",
                    "data": { "saved": saved, "needsReview": nr }
                })
                .to_string(),
            );
        },
    )
    .await;

    drop(progress_handle);

    let needs_review_count = needs_review_counter.load(std::sync::atomic::Ordering::Relaxed);
    let saved = saved_counter.load(std::sync::atomic::Ordering::Relaxed);
    let was_cancelled = is_enrich_cancelled();

    {
        let mut state = enrich_state.lock();
        state.progress.done = saved;
        state.progress.needs_review = needs_review_count;
    }

    if was_cancelled {
        // 中断：已完成的批次已落库，提示用户可重启续传
        emit_event(
            &event_tx,
            "enrich_cancelled",
            serde_json::json!({
                "saved": saved,
                "needsReview": needs_review_count,
                "total": results.len(),
            }),
        );
    } else {
        emit_event(
            &event_tx,
            "enrich_complete",
            serde_json::json!({
                "total": results.len(),
                "done": saved,
                "needsReview": needs_review_count,
            }),
        );
    }

    Ok(())
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

/// 从 DB 的 func_categories 表加载已启用功能分类的名称列表。
///
/// 这些名称（英文缩写如 Exp-Frameworks / Editor / FileMgr）作为 LLM prompt 的
/// `functional_category` 合法取值约束，确保 AI 返回的分类名与系统其余部分
/// （扫描器、分类器、建议引擎）使用的命名空间一致。
///
/// 历史 bug：早期版本从 categories.yaml 读取（中文名如"网络安全-漏洞利用"），
/// 与 DB 表的英文缩写命名空间不一致，导致 normalize_functional_category 无法对齐，
/// offline enricher 吐出的粗类（network/dev/Security）也无法归一化。
fn load_func_category_names(db: &CatalogDB) -> Vec<String> {
    db.get_enabled_func_categories()
        .unwrap_or_default()
        .into_iter()
        .map(|c| c.name)
        .collect()
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
