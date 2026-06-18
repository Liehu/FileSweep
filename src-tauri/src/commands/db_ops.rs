use tauri::State;

use crate::db::catalog::CatalogDB;

use serde_json::Value;

/// 清空所有数据库表（file_records, catalog_entries, operation_logs, tags, categories）。
///
/// 用于重置整个数据库。操作不可逆。
#[tauri::command]
pub async fn reset_db(db: State<'_, CatalogDB>) -> Result<(), String> {
    db.reset()
        .map_err(|e| format!("重置数据库失败: {}", e))
}

// ────────────────── Headless Wrappers ──────────────────

pub fn reset_db_headless(db: &CatalogDB) -> Result<Value, String> {
    db.reset()
        .map_err(|e| format!("重置数据库失败: {}", e))?;
    Ok(serde_json::json!({"ok": true}))
}
