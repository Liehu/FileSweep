use tauri::State;

use crate::core::executor::Reverter;
use crate::core::models::OperationLog;
use crate::db::catalog::CatalogDB;

use serde_json::Value;

/// 分页操作日志响应。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PaginatedLogs {
    pub logs: Vec<OperationLog>,
    pub total: i32,
}

/// 单次撤销操作的结果。
#[derive(Debug, Clone, serde::Serialize)]
pub struct RevertResult {
    pub id: i64,
    pub success: bool,
    pub error: Option<String>,
}

/// 分页查询操作日志，支持按操作类型、状态和关键词过滤。
#[tauri::command]
pub async fn get_logs(
    db: State<'_, CatalogDB>,
    page: i32,
    page_size: i32,
    action: Option<String>,
    status: Option<String>,
    q: Option<String>,
) -> Result<PaginatedLogs, String> {
    let action = action.unwrap_or_default();
    let status = status.unwrap_or_default();
    let q = q.unwrap_or_default();

    let (logs, total) = db
        .get_operation_logs("", &action, &status, &q, page, page_size)
        .map_err(|e| format!("查询操作日志失败: {}", e))?;

    Ok(PaginatedLogs { logs, total })
}

/// 撤销单个操作。
///
/// 根据 operation_logs 记录恢复被移动/删除的文件。
#[tauri::command]
pub async fn revert_operation(
    db: State<'_, CatalogDB>,
    id: i64,
) -> Result<(), String> {
    let log = db
        .get_operation_log_by_id(id)
        .map_err(|e| format!("查询操作日志失败: {}", e))?
        .ok_or_else(|| format!("操作日志不存在: {}", id))?;

    if !log.can_revert {
        return Err(format!("操作 {} 不支持撤销", id));
    }

    let mut reverter = Reverter::new(&db);
    reverter
        .revert(&log)
        .map_err(|e| format!("撤销操作失败: {}", e))?;

    db.mark_log_reverted(id)
        .map_err(|e| format!("标记日志已撤销失败: {}", e))?;

    Ok(())
}

/// 批量撤销多个操作，返回每条的结果。
#[tauri::command]
pub async fn batch_revert(
    db: State<'_, CatalogDB>,
    ids: Vec<i64>,
) -> Result<Vec<RevertResult>, String> {
    let mut reverter = Reverter::new(&db);
    let mut results = Vec::with_capacity(ids.len());

    for id in ids {
        let result = match db.get_operation_log_by_id(id) {
            Ok(Some(log)) => {
                if !log.can_revert {
                    RevertResult {
                        id,
                        success: false,
                        error: Some("不支持撤销".into()),
                    }
                } else {
                    match reverter.revert(&log) {
                        Ok(()) => {
                            let _ = db.mark_log_reverted(id);
                            RevertResult {
                                id,
                                success: true,
                                error: None,
                            }
                        }
                        Err(e) => RevertResult {
                            id,
                            success: false,
                            error: Some(e.to_string()),
                        },
                    }
                }
            }
            Ok(None) => RevertResult {
                id,
                success: false,
                error: Some("操作日志不存在".into()),
            },
            Err(e) => RevertResult {
                id,
                success: false,
                error: Some(format!("查询失败: {}", e)),
            },
        };
        results.push(result);
    }

    Ok(results)
}

// ────────────────── Headless 包装 ──────────────────

pub fn get_logs_headless(
    db: &CatalogDB, page: i32, page_size: i32,
    action: Option<String>, status: Option<String>, q: Option<String>,
) -> Result<Value, String> {
    let action = action.unwrap_or_default();
    let status = status.unwrap_or_default();
    let q = q.unwrap_or_default();

    let (logs, total) = db
        .get_operation_logs("", &action, &status, &q, page, page_size)
        .map_err(|e| format!("查询操作日志失败: {}", e))?;
    serde_json::to_value(PaginatedLogs { logs, total })
        .map_err(|e| format!("序列化失败: {}", e))
}

pub fn revert_operation_headless(db: &CatalogDB, id: i64) -> Result<Value, String> {
    let log = db
        .get_operation_log_by_id(id)
        .map_err(|e| format!("查询操作日志失败: {}", e))?
        .ok_or_else(|| format!("操作日志不存在: {}", id))?;

    if !log.can_revert {
        return Err(format!("操作 {} 不支持撤销", id));
    }

    let mut reverter = Reverter::new(db);
    reverter.revert(&log)
        .map_err(|e| format!("撤销操作失败: {}", e))?;
    db.mark_log_reverted(id)
        .map_err(|e| format!("标记日志已撤销失败: {}", e))?;

    Ok(Value::Null)
}

pub fn batch_revert_headless(db: &CatalogDB, ids: Vec<i64>) -> Result<Value, String> {
    let mut reverter = Reverter::new(db);
    let mut results = Vec::with_capacity(ids.len());

    for id in ids {
        let result = match db.get_operation_log_by_id(id) {
            Ok(Some(log)) => {
                if !log.can_revert {
                    RevertResult { id, success: false, error: Some("不支持撤销".into()) }
                } else {
                    match reverter.revert(&log) {
                        Ok(()) => {
                            let _ = db.mark_log_reverted(id);
                            RevertResult { id, success: true, error: None }
                        }
                        Err(e) => RevertResult { id, success: false, error: Some(e.to_string()) },
                    }
                }
            }
            Ok(None) => RevertResult { id, success: false, error: Some("操作日志不存在".into()) },
            Err(e) => RevertResult { id, success: false, error: Some(format!("查询失败: {}", e)) },
        };
        results.push(result);
    }

    serde_json::to_value(results).map_err(|e| format!("序列化失败: {}", e))
}
