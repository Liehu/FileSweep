use std::sync::Arc;
use tauri::{Emitter, State};

use crate::core::config::Config;
use crate::core::dedup::DedupDetector;
use crate::core::executor::{Executor, ExecutorAction, Operation};
use crate::core::models::FileRecord;
use crate::db::catalog::CatalogDB;

/// 清理操作的结果摘要。
#[derive(Debug, Clone, serde::Serialize)]
pub struct CleanResult {
    pub moved: usize,
    pub deleted: usize,
    pub failed: usize,
    pub dry_run: bool,
}

/// 启动异步清理任务。
///
/// 若前端传入了 `file_actions`，则按用户确认的操作执行；
/// 若为空，则自动从数据库获取活跃文件，运行去重检测后生成操作。
/// 完成后发射 `clean_complete` / `clean_error` 事件。
#[tauri::command]
pub async fn start_clean(
    app: tauri::AppHandle,
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    confirm: bool,
    file_actions: Vec<serde_json::Value>,
) -> Result<(), String> {
    let db = db.inner().clone();
    let config = config.inner().read().clone();

    tokio::spawn(async move {
        let dry_run = !confirm;

        // ── 解析或自动生成操作列表 ──
        let actions = if file_actions.is_empty() {
            // 自动模式：从 DB 获取文件 → 去重 → 生成操作
            match db.get_file_records("", "active", "", 1, 100_000) {
                Ok((records, _)) => generate_auto_actions(&records, &config),
                Err(e) => {
                    let _ = app.emit("clean_error", format!("查询文件失败: {}", e));
                    return;
                }
            }
        } else {
            parse_frontend_actions(&file_actions)
        };

        if actions.is_empty() {
            let _ = app.emit(
                "clean_complete",
                CleanResult {
                    moved: 0,
                    deleted: 0,
                    failed: 0,
                    dry_run,
                },
            );
            return;
        }

        // ── 创建执行器并执行 ──
        let scan_dir = if config.scan_dir.is_empty() {
            dirs::home_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_default()
        } else {
            config.scan_dir.clone()
        };

        let mut executor = Executor::new(dry_run, scan_dir.clone());
        let session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

        let logs = match executor.execute(&actions, &session_id) {
            Ok(logs) => logs,
            Err(e) => {
                let _ = app.emit("clean_error", format!("执行清理失败: {}", e));
                return;
            }
        };

        // 保存操作日志到数据库
        for op_log in &logs.logs {
            if let Err(e) = db.insert_operation_log(op_log) {
                log::error!("写入操作日志失败: {}", e);
            }
        }

        // 统计
        let mut moved = 0usize;
        let mut deleted = 0usize;
        let mut failed = 0usize;
        for op_log in &logs.logs {
            match op_log.status.as_str() {
                "success" | "dry_run" => {
                    if op_log.operation == "MOVE" {
                        moved += 1;
                    } else {
                        deleted += 1;
                    }
                }
                "error" => failed += 1,
                _ => {}
            }
        }

        let _ = app.emit(
            "clean_complete",
            CleanResult {
                moved,
                deleted,
                failed,
                dry_run,
            },
        );
    });

    Ok(())
}

/// headless 版清理：不依赖 Tauri State/AppHandle，通过 event_tx 广播事件。
///
/// 事件为 JSON 字符串：`{"event":"clean_complete|clean_error","data":...}`
/// 桥接层解析 event 字段后用原事件名 app.emit。
pub async fn start_clean_headless(
    db: Arc<CatalogDB>,
    config: Config,
    confirm: bool,
    file_actions: Vec<serde_json::Value>,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), String> {
    let dry_run = !confirm;

    let emit_event = |tx: &tokio::sync::broadcast::Sender<String>, event: &str, data: serde_json::Value| {
        let _ = tx.send(
            serde_json::json!({ "event": event, "data": data }).to_string(),
        );
    };

    // ── 解析或自动生成操作列表 ──
    let actions = if file_actions.is_empty() {
        match db.get_file_records("", "active", "", 1, 100_000) {
            Ok((records, _)) => generate_auto_actions(&records, &config),
            Err(e) => {
                emit_event(&event_tx, "clean_error", serde_json::json!(format!("查询文件失败: {}", e)));
                return Err(e);
            }
        }
    } else {
        parse_frontend_actions(&file_actions)
    };

    if actions.is_empty() {
        emit_event(
            &event_tx,
            "clean_complete",
            serde_json::to_value(CleanResult {
                moved: 0,
                deleted: 0,
                failed: 0,
                dry_run,
            })
            .unwrap_or_default(),
        );
        return Ok(());
    }

    // ── 创建执行器并执行 ──
    let scan_dir = if config.scan_dir.is_empty() {
        dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default()
    } else {
        config.scan_dir.clone()
    };

    let mut executor = Executor::new(dry_run, scan_dir.clone());
    let session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();

    let logs = match executor.execute(&actions, &session_id) {
        Ok(logs) => logs,
        Err(e) => {
            emit_event(&event_tx, "clean_error", serde_json::json!(format!("执行清理失败: {}", e)));
            return Err(e);
        }
    };

    // 保存操作日志到数据库
    for op_log in &logs.logs {
        if let Err(e) = db.insert_operation_log(op_log) {
            log::error!("写入操作日志失败: {}", e);
        }
    }

    // 统计
    let mut moved = 0usize;
    let mut deleted = 0usize;
    let mut failed = 0usize;
    for op_log in &logs.logs {
        match op_log.status.as_str() {
            "success" | "dry_run" => {
                if op_log.operation == "MOVE" {
                    moved += 1;
                } else {
                    deleted += 1;
                }
            }
            "error" => failed += 1,
            _ => {}
        }
    }

    emit_event(
        &event_tx,
        "clean_complete",
        serde_json::to_value(CleanResult {
            moved,
            deleted,
            failed,
            dry_run,
        })
        .unwrap_or_default(),
    );

    Ok(())
}
    let keep_newest = config.rules.keep_newest_version;

    let detector = DedupDetector::new(keep_newest, 2);
    let groups = detector.detect(records);

    let mut actions = Vec::new();
    for group in &groups {
        for dup in &group.duplicates {
            actions.push(ExecutorAction {
                operation: Operation::Delete,
                source: dup.local_path.clone(),
                dest: String::new(),
                reason: format!("重复文件 ({})", group.reason),
                file: dup.clone(),
            });
        }
    }
    actions
}

/// 解析前端传入的 JSON 操作列表为 ExecutorAction。
///
/// 每项 `file_action` 格式：
/// ```json
/// { "id": "...", "action": "keep|delete|move|archive", "move_target": "/path/..." }
/// ```
fn parse_frontend_actions(values: &[serde_json::Value]) -> Vec<ExecutorAction> {
    let mut actions = Vec::new();

    for val in values {
        let id = val["id"].as_str().unwrap_or_default().to_string();
        let action_str = val["action"].as_str().unwrap_or("keep");
        let move_target = val["move_target"].as_str().unwrap_or_default().to_string();

        let file = FileRecord {
            id: id.clone(),
            name: val["name"].as_str().unwrap_or_default().to_string(),
            local_path: val["local_path"].as_str().unwrap_or_default().to_string(),
            file_hash: val["file_hash"].as_str().unwrap_or_default().to_string(),
            file_size: val["file_size"].as_i64().unwrap_or(0),
            extension: val["extension"].as_str().unwrap_or_default().to_string(),
            is_app_dir: val["is_app_dir"].as_bool().unwrap_or(false),
            app_dir_path: val["app_dir_path"].as_str().unwrap_or_default().to_string(),
            ..Default::default()
        };

        let (operation, dest) = match action_str {
            "delete" => (Operation::Delete, String::new()),
            "move" => (Operation::Move, move_target),
            "archive" => (Operation::Move, move_target),
            _ => continue, // "keep" → 跳过
        };

        actions.push(ExecutorAction {
            operation,
            source: file.local_path.clone(),
            dest,
            reason: format!("用户手动操作: {}", action_str),
            file,
        });
    }

    actions
}
