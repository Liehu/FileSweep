use std::sync::Arc;
use tauri::State;

use crate::db::catalog::{CatalogDB, TagEntry};

use serde_json::Value;

/// 查询所有标签（含每个标签关联的条目计数）。
#[tauri::command]
pub async fn get_tags(db: State<'_, Arc<CatalogDB>>) -> Result<Vec<TagEntry>, String> {
    db.get_tags()
        .map_err(|e| format!("查询标签失败: {}", e))
}

/// 创建新标签。
#[tauri::command]
pub async fn create_tag(
    db: State<'_, Arc<CatalogDB>>,
    name: String,
    color: String,
    description: String,
) -> Result<TagEntry, String> {
    let tag = TagEntry {
        id: String::new(), // 由 DB 层自动生成
        name,
        color,
        description,
        count: 0,
    };

    db.insert_tag(&tag)
        .map_err(|e| format!("创建标签失败: {}", e))
}

/// 更新指定标签的名称、颜色和描述。
#[tauri::command]
pub async fn update_tag(
    db: State<'_, Arc<CatalogDB>>,
    id: String,
    name: String,
    color: String,
    description: String,
) -> Result<(), String> {
    let tag = TagEntry {
        id,
        name,
        color,
        description,
        count: 0, // 不影响更新
    };

    db.update_tag(&tag)
        .map_err(|e| format!("更新标签失败: {}", e))
}

/// 删除指定标签。
#[tauri::command]
pub async fn delete_tag(
    db: State<'_, Arc<CatalogDB>>,
    id: String,
) -> Result<(), String> {
    db.delete_tag(&id)
        .map_err(|e| format!("删除标签失败: {}", e))
}

// ────────────────── Headless Wrappers ──────────────────

pub fn get_tags_headless(db: &CatalogDB) -> Result<Value, String> {
    let tags = db.get_tags().map_err(|e| format!("查询标签失败: {}", e))?;
    serde_json::to_value(tags).map_err(|e| format!("序列化失败: {}", e))
}

pub fn create_tag_headless(
    db: &CatalogDB,
    name: String,
    color: String,
    description: String,
) -> Result<Value, String> {
    let tag = TagEntry {
        id: String::new(),
        name,
        color,
        description,
        count: 0,
    };
    let result = db.insert_tag(&tag).map_err(|e| format!("创建标签失败: {}", e))?;
    serde_json::to_value(result).map_err(|e| format!("序列化失败: {}", e))
}

pub fn update_tag_headless(db: &CatalogDB, body: Value) -> Result<Value, String> {
    let id = body["id"].as_str().unwrap_or_default().to_string();
    let name = body["name"].as_str().unwrap_or_default().to_string();
    let color = body["color"].as_str().unwrap_or_default().to_string();
    let description = body["description"].as_str().unwrap_or_default().to_string();

    let tag = TagEntry {
        id,
        name,
        color,
        description,
        count: 0,
    };
    db.update_tag(&tag)
        .map_err(|e| format!("更新标签失败: {}", e))?;
    Ok(serde_json::json!({"ok": true}))
}

pub fn delete_tag_headless(db: &CatalogDB, id: String) -> Result<Value, String> {
    db.delete_tag(&id).map_err(|e| format!("删除标签失败: {}", e))?;
    Ok(serde_json::json!({"ok": true}))
}
