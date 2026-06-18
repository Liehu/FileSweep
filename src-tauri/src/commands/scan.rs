use std::sync::Arc;
use tauri::{Emitter, State};

use crate::core::classifier::Classifier;
use crate::core::config::Config;
use crate::core::dedup::DedupDetector;
use crate::core::models::{FileRecord, ScanProgress};
use crate::core::scanner::Scanner;
use crate::db::catalog::{CatalogDB, FileStats};

use serde_json::Value;

// ────────────────── 响应结构体 ──────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct PaginatedFiles {
    pub files: Vec<FileRecord>,
    pub total: i32,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileSuggestion {
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub suggestion_type: String,
    pub reason: String,
    pub keep_id: Option<String>,
    pub keep_name: Option<String>,
    pub action: String,
}

// ────────────────── Tauri Commands ──────────────────

/// 启动异步扫描任务。
///
/// 后台流程：加载分类规则 → 逐目录扫描 → 过滤排除项 → 分类文件 → 批量写入 DB → 去重检测，
/// 全程通过 `AppHandle` 发射 `scan_progress` / `scan_complete` / `scan_error` 事件。
#[tauri::command]
pub async fn start_scan(
    app: tauri::AppHandle,
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    dirs: Vec<String>,
    recursive: bool,
    exclude_dirs: Vec<String>,
    exclude_names: Vec<String>,
    exclude_exts: Vec<String>,
    detect_app_dirs: bool,
) -> Result<(), String> {
    let db = db.inner().clone();
    let config = config.inner().read().clone();

    tokio::spawn(async move {
        let total_dirs = dirs.len();
        let mut all_records: Vec<FileRecord> = Vec::new();

        // ── 创建进度通道 ──
        let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<ScanProgress>();

        // spawn 一个独立任务转发进度事件到前端
        let app_clone = app.clone();
        tokio::spawn(async move {
            while let Some(p) = progress_rx.recv().await {
                let _ = app_clone.emit("scan_progress", p);
            }
        });

        // ── 1. 加载分类规则 ──
        let rules_path = config.rules_path.clone();
        let classifier = match Classifier::new(&rules_path) {
            Ok(c) => c,
            Err(e) => {
                log::warn!("加载分类规则失败: {}, 将使用默认规则", e);
                Classifier::with_defaults()
            }
        };

        // ── 2. 逐目录扫描 ──
        let scanner = Scanner::new();

        for (idx, dir) in dirs.iter().enumerate() {
            log::info!("开始扫描目录 {}: {}", idx + 1, dir);
            let _ = progress_tx.send(ScanProgress {
                total: 0,
                done: 0,
                current_file: format!("扫描目录: {}", dir),
                stage: "walking".into(),
            });

            match scanner.scan(dir, recursive, detect_app_dirs, Some(progress_tx.clone())).await {
                Ok(mut records) => {
                    log::info!("目录 {} 扫描到 {} 个文件", dir, records.len());
                    // 按排除列表过滤
                    records.retain(|r| {
                        if !exclude_dirs.is_empty() {
                            for exc in &exclude_dirs {
                                if r.local_path.contains(exc) {
                                    return false;
                                }
                            }
                        }
                        if !exclude_names.is_empty() {
                            let name_lower = r.name.to_lowercase();
                            for exc in &exclude_names {
                                if name_lower.contains(&exc.to_lowercase()) {
                                    return false;
                                }
                            }
                        }
                        if !exclude_exts.is_empty() {
                            let ext_lower = r.extension.to_lowercase();
                            for exc in &exclude_exts {
                                if ext_lower == exc.to_lowercase() {
                                    return false;
                                }
                            }
                        }
                        true
                    });

                    for record in &mut records {
                        let result = classifier.classify(record);
                        record.category = result.category;
                    }

                    let count = records.len();
                    all_records.extend(records);

                    let _ = progress_tx.send(ScanProgress {
                        total: total_dirs,
                        done: idx + 1,
                        current_file: format!("已扫描 {} 个文件", count),
                        stage: "scanned".into(),
                    });
                }
                Err(e) => {
                    let _ = app.emit("scan_error", format!("扫描目录 {} 失败: {}", dir, e));
                    return;
                }
            }
        }

        // ── 3. 写入数据库 ──
        let _ = progress_tx.send(ScanProgress {
            total: all_records.len(),
            done: 0,
            current_file: "写入数据库中...".into(),
            stage: "saving".into(),
        });

        if let Err(e) = db.batch_insert_file_records(&all_records) {
            let _ = app.emit("scan_error", format!("保存扫描结果失败: {}", e));
            log::error!("保存扫描结果失败: {}", e);
            return;
        }
        log::info!("扫描完成，共写入 {} 条记录", all_records.len());

        // ── 4. 去重检测 ──
        let _ = progress_tx.send(ScanProgress {
            total: all_records.len(),
            done: all_records.len(),
            current_file: "去重检测中...".into(),
            stage: "dedup".into(),
        });

        let keep_newest = config.rules.keep_newest_version;
        let detector = DedupDetector::new(keep_newest, 2);
        let groups = detector.detect(&all_records);
        let dup_count: usize = groups.iter().map(|g| g.duplicates.len()).sum();

        // ── 5. 发射完成事件 ──
        let _ = app.emit(
            "scan_complete",
            serde_json::json!({
                "totalFiles": all_records.len(),
                "dedupGroups": groups.len(),
                "duplicates": dup_count,
            }),
        );
    });

    Ok(())
}

/// 分页查询文件列表，支持按分类、状态和关键词过滤。
#[tauri::command]
pub async fn get_files(
    db: State<'_, Arc<CatalogDB>>,
    page: i32,
    page_size: i32,
    category: Option<String>,
    status: Option<String>,
    search: Option<String>,
) -> Result<PaginatedFiles, String> {
    let category = category.unwrap_or_default();
    let status = status.unwrap_or_default();
    let search = search.unwrap_or_default();

    let (files, total) = db
        .get_file_records(&category, &status, &search, page, page_size)
        .map_err(|e| format!("查询文件记录失败: {}", e))?;

    Ok(PaginatedFiles { files, total })
}

/// 获取文件统计信息（总数、总大小、重复数、多版本数、未分类数）。
#[tauri::command]
pub async fn get_file_stats(db: State<'_, Arc<CatalogDB>>) -> Result<FileStats, String> {
    db.get_file_stats()
        .map_err(|e| format!("获取文件统计失败: {}", e))
}

/// 运行去重检测，为每个重复/旧版文件返回建议操作。
#[tauri::command]
pub async fn get_suggestions(
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<Config>>,
) -> Result<Vec<FileSuggestion>, String> {
    let (records, _) = db
        .get_file_records("", "active", "", 1, 100_000)
        .map_err(|e| format!("查询文件记录失败: {}", e))?;

    let keep_newest = config.rules.keep_newest_version;
    let detector = DedupDetector::new(keep_newest, 2);
    let groups = detector.detect(&records);

    let mut suggestions = Vec::new();
    for group in &groups {
        let rep = &group.representative;
        for dup in &group.duplicates {
            let action = match group.reason.as_str() {
                "redundant_archive" => "archive",
                "multi_version" => "delete",
                _ => "delete",
            };

            suggestions.push(FileSuggestion {
                file_id: dup.id.clone(),
                file_name: dup.name.clone(),
                file_path: dup.local_path.clone(),
                suggestion_type: group.reason.clone(),
                reason: format!("与 {} 重复 ({})", rep.name, group.reason),
                keep_id: Some(rep.id.clone()),
                keep_name: Some(rep.name.clone()),
                action: action.into(),
            });
        }
    }

    Ok(suggestions)
}

// ────────────────── Headless Wrappers ──────────────────

pub async fn start_scan_headless(
    db: CatalogDB,
    config: Arc<Config>,
    dirs: Vec<String>,
    recursive: bool,
    exclude_dirs: Vec<String>,
    exclude_names: Vec<String>,
    exclude_exts: Vec<String>,
    detect_app_dirs: bool,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<Value, String> {
    let total_dirs = dirs.len();
    let mut all_records: Vec<FileRecord> = Vec::new();

    let (progress_tx, mut progress_rx) = tokio::sync::mpsc::unbounded_channel::<ScanProgress>();

    // 将扫描进度转发到 SSE 广播器
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        while let Some(p) = progress_rx.recv().await {
            let data = serde_json::to_string(&p).unwrap_or_default();
            let _ = event_tx_clone.send(format!("{{\"event\":\"scan_progress\",\"data\":{}}}", data));
        }
    });

    // 1. 加载分类规则
    let rules_path = config.rules_path.clone();
    let classifier = match Classifier::new(&rules_path) {
        Ok(c) => c,
        Err(e) => {
            log::warn!("加载分类规则失败: {}, 将使用默认规则", e);
            Classifier::with_defaults()
        }
    };

    // 2. 逐目录扫描
    let scanner = Scanner::new();
    for (idx, dir) in dirs.iter().enumerate() {
        log::info!("开始扫描目录 {}: {}", idx + 1, dir);
        let _ = progress_tx.send(ScanProgress {
            total: 0,
            done: 0,
            current_file: format!("扫描目录: {}", dir),
            stage: "walking".into(),
        });

        match scanner.scan(dir, recursive, detect_app_dirs, Some(progress_tx.clone())).await {
            Ok(mut records) => {
                log::info!("目录 {} 扫描到 {} 个文件", dir, records.len());
                records.retain(|r| {
                    if !exclude_dirs.is_empty() {
                        for exc in &exclude_dirs {
                            if r.local_path.contains(exc) {
                                return false;
                            }
                        }
                    }
                    if !exclude_names.is_empty() {
                        let name_lower = r.name.to_lowercase();
                        for exc in &exclude_names {
                            if name_lower.contains(&exc.to_lowercase()) {
                                return false;
                            }
                        }
                    }
                    if !exclude_exts.is_empty() {
                        let ext_lower = r.extension.to_lowercase();
                        for exc in &exclude_exts {
                            if ext_lower == exc.to_lowercase() {
                                return false;
                            }
                        }
                    }
                    true
                });

                for record in &mut records {
                    let result = classifier.classify(record);
                    record.category = result.category;
                }

                let count = records.len();
                all_records.extend(records);

                let _ = progress_tx.send(ScanProgress {
                    total: total_dirs,
                    done: idx + 1,
                    current_file: format!("已扫描 {} 个文件", count),
                    stage: "scanned".into(),
                });
            }
            Err(e) => {
                log::error!("扫描目录 {} 失败: {}", dir, e);
                return Err(format!("扫描目录 {} 失败: {}", dir, e));
            }
        }
    }

    // 3. 写入数据库
    if let Err(e) = db.batch_insert_file_records(&all_records) {
        log::error!("保存扫描结果失败: {}", e);
        return Err(format!("保存扫描结果失败: {}", e));
    }
    log::info!("扫描完成，共写入 {} 条记录", all_records.len());

    // 4. 去重检测
    let keep_newest = config.rules.keep_newest_version;
    let detector = DedupDetector::new(keep_newest, 2);
    let groups = detector.detect(&all_records);
    let dup_count: usize = groups.iter().map(|g| g.duplicates.len()).sum();

    Ok(serde_json::json!({
        "totalFiles": all_records.len(),
        "dedupGroups": groups.len(),
        "duplicates": dup_count,
    }))
    // 发射 scan_complete 事件
    .map(|v| {
        let _ = event_tx.send(format!("{{\"event\":\"scan_complete\",\"data\":{}}}", v));
        v
    })
}

pub fn get_files_headless(
    db: &CatalogDB,
    page: i32,
    page_size: i32,
    category: Option<String>,
    status: Option<String>,
    search: Option<String>,
) -> Result<Value, String> {
    let category = category.unwrap_or_default();
    let status = status.unwrap_or_default();
    let search = search.unwrap_or_default();

    let (files, total) = db
        .get_file_records(&category, &status, &search, page, page_size)
        .map_err(|e| format!("查询文件记录失败: {}", e))?;

    serde_json::to_value(PaginatedFiles { files, total })
        .map_err(|e| format!("序列化失败: {}", e))
}

pub fn get_file_stats_headless(db: &CatalogDB) -> Result<Value, String> {
    let stats = db
        .get_file_stats()
        .map_err(|e| format!("获取文件统计失败: {}", e))?;
    serde_json::to_value(stats).map_err(|e| format!("序列化失败: {}", e))
}

pub async fn get_suggestions_headless(
    db: &CatalogDB,
    config: &Arc<tokio::sync::RwLock<Config>>,
) -> Result<Value, String> {
    let (records, _) = db
        .get_file_records("", "active", "", 1, 100_000)
        .map_err(|e| format!("查询文件记录失败: {}", e))?;

    let cfg = config.read().await;
    let keep_newest = cfg.rules.keep_newest_version;
    let detector = DedupDetector::new(keep_newest, 2);
    let groups = detector.detect(&records);

    let mut suggestions = Vec::new();
    for group in &groups {
        let rep = &group.representative;
        for dup in &group.duplicates {
            let action = match group.reason.as_str() {
                "redundant_archive" => "archive",
                "multi_version" => "delete",
                _ => "delete",
            };
            suggestions.push(FileSuggestion {
                file_id: dup.id.clone(),
                file_name: dup.name.clone(),
                file_path: dup.local_path.clone(),
                suggestion_type: group.reason.clone(),
                reason: format!("与 {} 重复 ({})", rep.name, group.reason),
                keep_id: Some(rep.id.clone()),
                keep_name: Some(rep.name.clone()),
                action: action.into(),
            });
        }
    }

    serde_json::to_value(suggestions).map_err(|e| format!("序列化失败: {}", e))
}
