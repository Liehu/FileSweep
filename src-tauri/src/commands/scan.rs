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
        let software_roots: Vec<String> = db.get_enabled_software_roots().unwrap_or_default();
        let dir_patterns: Vec<crate::db::config::DirPatternRow> = db.get_enabled_dir_patterns().unwrap_or_default();
        // 合并 DB 默认排除规则 + 前端传入的排除项
        let db_excludes = db.get_enabled_exclude_rules().unwrap_or_default();
        let mut all_exclude_dirs = exclude_dirs.clone();
        for d in &db_excludes.dirs {
            if !all_exclude_dirs.iter().any(|x| x.eq_ignore_ascii_case(d)) {
                all_exclude_dirs.push(d.clone());
            }
        }
        let scanner = Scanner::new();

        for (idx, dir) in dirs.iter().enumerate() {
            let is_sw_root = is_path_in_software_roots(dir, &software_roots);
            log::info!("开始扫描目录 {}: {} (software_root={})", idx + 1, dir, is_sw_root);
            let _ = progress_tx.send(ScanProgress::indeterminate(
                "walking",
                "扫描目录",
                0,
                format!("扫描目录: {}", dir),
            ));

            let scan_result = if is_sw_root {
                scanner.scan_software_root(dir, Some(progress_tx.clone())).await
            } else {
                scanner.scan(dir, recursive, detect_app_dirs, &dir_patterns, &all_exclude_dirs, Some(progress_tx.clone())).await
            };

            match scan_result {
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

                    let func_cats: Vec<crate::db::config::FuncCategoryRow> = if config.enable_func_classify {
                        db.get_enabled_func_categories().unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    for record in &mut records {
                        let result = classifier.classify(record);
                        record.category = result.category;
                        if config.enable_func_classify {
                            record.functional_category =
                                crate::core::classifier::Classifier::classify_functional(record, &func_cats)
                                    .unwrap_or_default();
                        }
                    }

                    let count = records.len();
                    all_records.extend(records);

                    let _ = progress_tx.send(ScanProgress::determinate(
                        "scanned",
                        "扫描目录",
                        total_dirs,
                        idx + 1,
                        format!("已扫描 {} 个文件", count),
                        0.0,
                        0,
                    ));
                }
                Err(e) => {
                    let _ = app.emit("scan_error", format!("扫描目录 {} 失败: {}", dir, e));
                    return;
                }
            }
        }

        // ── 3. 写入数据库 ──
        let _ = progress_tx.send(ScanProgress::indeterminate(
            "saving",
            "写入数据库",
            0,
            "写入数据库中...".to_string(),
        ));

        if let Err(e) = db.batch_insert_file_records(&all_records, "") {
            let _ = app.emit("scan_error", format!("保存扫描结果失败: {}", e));
            log::error!("保存扫描结果失败: {}", e);
            return;
        }
        log::info!("扫描完成，共写入 {} 条记录", all_records.len());

        // ── 4. 去重检测 ──
        let _ = progress_tx.send(ScanProgress::indeterminate(
            "dedup",
            "去重检测",
            0,
            "去重检测中...".to_string(),
        ));

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

/// 扫描取消标志（模块级，scan:cancel action 设置为 true）
static SCAN_CANCEL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 扫描完成标志（scan:start 开始时 false，完成时 true）
static SCAN_COMPLETE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// 请求取消当前扫描
pub fn request_scan_cancel() {
    SCAN_CANCEL.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// 检查扫描是否被取消
pub fn is_scan_cancelled() -> bool {
    SCAN_CANCEL.load(std::sync::atomic::Ordering::SeqCst)
}

/// 检查扫描是否完成
pub fn is_scan_complete() -> bool {
    SCAN_COMPLETE.load(std::sync::atomic::Ordering::SeqCst)
}

pub async fn start_scan_headless(
    db: Arc<CatalogDB>,
    config: Arc<Config>,
    dirs: Vec<String>,
    recursive: bool,
    exclude_dirs: Vec<String>,
    exclude_names: Vec<String>,
    exclude_exts: Vec<String>,
    detect_app_dirs: bool,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<Value, String> {
    // 重置取消标志 + 完成标志
    SCAN_CANCEL.store(false, std::sync::atomic::Ordering::SeqCst);
    SCAN_COMPLETE.store(false, std::sync::atomic::Ordering::SeqCst);

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
    let software_roots: Vec<String> = db.get_enabled_software_roots().unwrap_or_default();
    let dir_patterns: Vec<crate::db::config::DirPatternRow> = db.get_enabled_dir_patterns().unwrap_or_default();
    // 合并 DB 默认排除规则（dir 类型）+ 前端传入的排除项，用于遍历时跳过噪音目录
    let db_excludes = db.get_enabled_exclude_rules().unwrap_or_default();
    let mut all_exclude_dirs = exclude_dirs.clone();
    for d in &db_excludes.dirs {
        if !all_exclude_dirs.iter().any(|x| x.eq_ignore_ascii_case(d)) {
            all_exclude_dirs.push(d.clone());
        }
    }
    let scanner = Scanner::new();
    for (idx, dir) in dirs.iter().enumerate() {
        if is_scan_cancelled() {
            log::info!("扫描已被用户取消");
            let _ = event_tx.send(format!("{{\"event\":\"scan_cancelled\",\"data\":{{}}}}"));
            return Ok(serde_json::json!({"cancelled": true}));
        }

        let is_software_root = is_path_in_software_roots(dir, &software_roots);
        log::info!("开始扫描目录 {}: {}", idx + 1, dir);
        let _ = progress_tx.send(ScanProgress::indeterminate(
            "walking",
            "扫描目录",
            0,
            format!("扫描目录: {}", dir),
        ));

        let scan_result = if is_software_root {
            scanner.scan_software_root(dir, Some(progress_tx.clone())).await
        } else {
            scanner.scan(dir, recursive, detect_app_dirs, &dir_patterns, &all_exclude_dirs, Some(progress_tx.clone())).await
        };

        match scan_result {
            Ok(mut records) => {
                let app_dir_count = records.iter().filter(|r| r.is_app_dir).count();
                log::info!("目录 {} 扫描到 {} 个文件（app dir: {}，普通: {}）",
                    dir, records.len(), app_dir_count, records.len() - app_dir_count);
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

                // 功能分类（开关控制）：开启时查 func_categories，对每个文件做 token 关键词匹配
                let func_cats: Vec<crate::db::config::FuncCategoryRow> = if config.enable_func_classify {
                    db.get_enabled_func_categories().unwrap_or_default()
                } else {
                    Vec::new()
                };
                for record in &mut records {
                    let result = classifier.classify(record);
                    record.category = result.category;
                    if config.enable_func_classify {
                        record.functional_category =
                            crate::core::classifier::Classifier::classify_functional(record, &func_cats)
                                .unwrap_or_default();
                    }
                }

                let count = records.len();
                all_records.extend(records);

                let _ = progress_tx.send(ScanProgress::determinate(
                    "scanned",
                    "扫描目录",
                    total_dirs,
                    idx + 1,
                    format!("已扫描 {} 个文件", count),
                    0.0,
                    0,
                ));
            }
            Err(e) => {
                log::error!("扫描目录 {} 失败: {}", dir, e);
                return Err(format!("扫描目录 {} 失败: {}", dir, e));
            }
        }
    }

    // 3. 写入数据库（含扫描任务记录）
    let _ = progress_tx.send(ScanProgress::indeterminate(
        "saving",
        "写入数据库",
        0,
        format!("正在写入 {} 条记录...", all_records.len()),
    ));
    let task_id = format!("task_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let started_at = chrono::Utc::now().to_rfc3339();
    let scan_dirs_joined = dirs.join("; ");
    if let Err(e) = db.insert_scan_task(&crate::core::models::ScanTask {
        id: task_id.clone(),
        scan_dir: scan_dirs_joined.clone(),
        started_at: started_at.clone(),
        finished_at: String::new(),
        file_count: all_records.len() as i64,
        status: "running".to_string(),
        recursive,
    }) {
        log::warn!("写入扫描任务记录失败: {}", e);
    }

    if let Err(e) = db.batch_insert_file_records(&all_records, &task_id) {
        log::error!("保存扫描结果失败: {}", e);
        let _ = event_tx.send(format!("{{\"event\":\"scan_complete\",\"data\":{{\"error\":\"{}\"}}}}", e));
        return Err(format!("保存扫描结果失败: {}", e));
    }
    log::info!("扫描完成，共写入 {} 条记录", all_records.len());

    // 4. 去重检测：把重复/多版本文件的状态写回 DB，供前端按 status 筛选
    //    （dedup 是内存计算，结果必须持久化到 file_records.status 才能在筛选页显示）
    mark_dedup_status(&db, &all_records, &config);

    // 5. 更新任务记录为完成
    let _ = db.insert_scan_task(&crate::core::models::ScanTask {
        id: task_id.clone(),
        scan_dir: scan_dirs_joined,
        started_at,
        finished_at: chrono::Utc::now().to_rfc3339(),
        file_count: all_records.len() as i64,
        status: "done".to_string(),
        recursive,
    });

    // 6. 保存目录快照（供下次增量扫描 diff）
    save_scan_snapshot(&config, &dirs, &all_records);

    let complete_data = serde_json::json!({
        "totalFiles": all_records.len(),
    });
    SCAN_COMPLETE.store(true, std::sync::atomic::Ordering::SeqCst);
    let _ = event_tx.send(format!(
        "{{\"event\":\"scan_complete\",\"data\":{}}}",
        complete_data
    ));

    Ok(complete_data)
}

pub fn get_files_headless(
    db: &CatalogDB,
    page: i32,
    page_size: i32,
    category: Option<String>,
    status: Option<String>,
    search: Option<String>,
    dir_type: Option<String>,
    task_id: Option<String>,
) -> Result<Value, String> {
    let category = category.unwrap_or_default();
    let status = status.unwrap_or_default();
    let search = search.unwrap_or_default();
    let dir_type = dir_type.unwrap_or_default();
    let task_id = task_id.unwrap_or_default();

    let (files, total) = db
        .get_file_records_filtered(&category, &status, &search, &dir_type, &task_id, page, page_size)
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

/// 判断扫描路径是否匹配 software_roots 表中的某个路径。
///
/// 匹配规则（Windows 不区分大小写，忽略尾斜杠）：
/// - 精确匹配：scan_dir == root
/// - 根目录的父级匹配：scan_dir 是 root 下的子路径也视为 software_root
///   （但实际扫描时 scan_software_root 只 read_dir 一层，所以这里只判断精确匹配 + root 本身）
fn is_path_in_software_roots(scan_dir: &str, software_roots: &[String]) -> bool {
    let normalize = |p: &str| -> String {
        p.trim_end_matches(['\\', '/']).to_lowercase().replace('/', "\\")
    };
    let target = normalize(scan_dir);
    software_roots.iter().any(|r| normalize(r) == target)
}

/// 保存扫描快照（供下次增量扫描 diff）。
///
/// 对每个扫描目录生成一个快照文件（路径→size+mtime+hash），
/// 下次扫描同目录时可 diff 复用未变更文件的 hash。
fn save_scan_snapshot(config: &Config, scan_dirs: &[String], records: &[FileRecord]) {
    let db_path = std::path::Path::new(&config.db_path);
    let data_dir = db_path.parent().unwrap_or_else(|| std::path::Path::new("."));

    for dir_str in scan_dirs {
        let snap_path = crate::core::snapshot::DirSnapshot::snapshot_path(data_dir, dir_str);
        let mut snap = crate::core::snapshot::DirSnapshot::default();
        snap.root = dir_str.clone();

        let dir_prefix = std::path::Path::new(dir_str);
        for r in records {
            // 只记录该目录下的文件（用路径前缀匹配）
            let fpath = std::path::Path::new(&r.local_path);
            if let Ok(rel) = fpath.strip_prefix(dir_prefix) {
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let mtime_nanos = r.mod_time.timestamp_nanos_opt().unwrap_or(0) as u64;
                snap.files.insert(
                    rel_str,
                    crate::core::snapshot::FileSnapshot {
                        size: r.file_size as u64,
                        mtime_nanos,
                        hash: r.file_hash.clone(),
                        category: r.category.clone(),
                    },
                );
            }
        }

        if let Err(e) = snap.save(&snap_path) {
            log::warn!("保存快照失败 {} -> {:?}: {}", dir_str, snap_path, e);
        } else {
            log::info!("已保存快照：{}（{} 个文件）", dir_str, snap.files.len());
        }
    }
}

/// 跑去重检测，把重复/多版本文件的状态写回 file_records.status。
///
/// 前端「重复文件」「多版本」筛选页按 status 查询：
/// - status="duplicate"：精确哈希/冗余压缩包/大小/模糊名称匹配的重复
/// - status="multiversion"：版本号不同的多版本
/// - 其余保持 "active"
///
/// 必须在 batch_insert 后调用，否则 dedup 结果无法持久化。
fn mark_dedup_status(db: &CatalogDB, records: &[FileRecord], config: &Config) {
    let keep_newest = config.rules.keep_newest_version;
    let detector = DedupDetector::new(keep_newest, 2);
    let groups = detector.detect(records);

    let mut marked = 0usize;
    for group in &groups {
        // 代表项保持 active，其余重复项按 reason 标记
        let status = if group.reason == "multi_version" {
            "multiversion"
        } else {
            "duplicate"
        };
        for dup in &group.duplicates {
            if let Err(e) = db.update_file_status(&dup.id, status) {
                log::warn!("更新文件状态失败 {} -> {}: {}", dup.id, status, e);
            } else {
                marked += 1;
            }
        }
    }
    if marked > 0 {
        log::info!("去重标记完成：{} 个文件标记为 duplicate/multiversion", marked);
    }
}
