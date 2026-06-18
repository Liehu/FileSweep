use tauri::State;

use std::sync::Arc;
use crate::core::models::CatalogEntry;
use crate::db::catalog::CatalogDB;

use serde_json::Value;

/// 分页目录查询响应。
#[derive(Debug, Clone, serde::Serialize)]
pub struct PaginatedCatalog {
    pub entries: Vec<CatalogEntry>,
    pub total: i32,
}

/// 分页查询目录条目，支持按关键词搜索（名称/描述/标签）。
#[tauri::command]
pub async fn get_catalog(
    db: State<'_, CatalogDB>,
    page: i32,
    page_size: i32,
    search: Option<String>,
) -> Result<PaginatedCatalog, String> {
    let search = search.unwrap_or_default();

    let (entries, total) = db
        .get_catalog_entries(&search, page, page_size)
        .map_err(|e| format!("查询目录条目失败: {}", e))?;

    Ok(PaginatedCatalog { entries, total })
}

/// 更新指定目录条目的字段。
///
/// 仅更新传入的非 None 字段；未传入的字段保持原值。
#[tauri::command]
pub async fn update_catalog_entry(
    db: State<'_, CatalogDB>,
    id: String,
    description: Option<String>,
    homepage_url: Option<String>,
    download_url: Option<String>,
    latest_version: Option<String>,
    license: Option<String>,
    functional_category: Option<String>,
    tags: Option<Vec<String>>,
    notes: Option<String>,
    needs_review: Option<bool>,
) -> Result<(), String> {
    // 先读取现有条目
    let mut entry = db
        .get_catalog_entry_by_id(&id)
        .map_err(|e| format!("查询目录条目失败: {}", e))?
        .ok_or_else(|| format!("目录条目不存在: {}", id))?;

    // 仅覆盖非 None 字段
    if let Some(v) = description {
        entry.description = v;
    }
    if let Some(v) = homepage_url {
        entry.homepage_url = v;
    }
    if let Some(v) = download_url {
        entry.download_url = v;
    }
    if let Some(v) = latest_version {
        entry.latest_version = v;
    }
    if let Some(v) = license {
        entry.license = v;
    }
    if let Some(v) = functional_category {
        entry.functional_category = v;
    }
    if let Some(v) = tags {
        entry.tags = v;
    }
    if let Some(v) = notes {
        entry.notes = v;
    }
    if let Some(v) = needs_review {
        entry.needs_review = v;
    }

    db.update_catalog_entry(&entry)
        .map_err(|e| format!("更新目录条目失败: {}", e))
}

/// 删除指定目录条目。
#[tauri::command]
pub async fn delete_catalog_entry(
    db: State<'_, CatalogDB>,
    id: String,
) -> Result<(), String> {
    db.delete_catalog_entry(&id)
        .map_err(|e| format!("删除目录条目失败: {}", e))
}

/// 将目录数据导出为指定格式（CSV 或 Obsidian Markdown），返回字符串内容。
#[tauri::command]
pub async fn export_catalog(
    db: State<'_, CatalogDB>,
    format: String,
) -> Result<Value, String> {
    let (entries, _total) = db
        .get_catalog_entries("", 1, 1_000_000)
        .map_err(|e| format!("查询目录条目失败: {}", e))?;

    let output = match format.to_lowercase().as_str() {
        "csv" => export_csv(&entries),
        "obsidian" | "markdown" | "md" | "obsidian-md" => export_obsidian_md(&entries),
        _ => return Err(format!("不支持的导出格式: {}", format)),
    };

    Ok(serde_json::json!({"content": output, "format": format}))
}

// ────────────────── 导出格式实现 ──────────────────

fn export_csv(entries: &[CatalogEntry]) -> String {
    let mut csv = String::from(
        "ID,Name,Description,Homepage,Download,Version,License,Category,Tags,Confidence,NeedsReview\n",
    );
    for e in entries {
        let tags_str = e.tags.join(";");
        let needs_review = if e.needs_review { "true" } else { "false" };
        csv.push_str(&format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",{},{}\n",
            e.id,
            e.name,
            e.description.replace('"', "\"\""),
            e.homepage_url,
            e.download_url,
            e.latest_version,
            e.license,
            e.functional_category,
            tags_str,
            e.ai_confidence,
            needs_review,
        ));
    }
    csv
}

fn export_obsidian_md(entries: &[CatalogEntry]) -> String {
    let mut md = String::new();
    for e in entries {
        let tags_array = e
            .tags
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(", ");

        md.push_str(&format!(
            "---\n\
             id: {}\n\
             name: \"{}\"\n\
             category: {}\n\
             tags: [{}]\n\
             confidence: {}\n\
             needs_review: {}\n\
             ---\n\
             ## {}\n\
             \n\
             - **描述**: {}\n\
             - **版本**: {}\n\
             - **主页**: [链接]({})\n\
             - **下载**: [链接]({})\n\
             - **许可证**: {}\n\
             - **AI 提供方**: {}\n\
             \n\n",
            e.id,
            e.name,
            e.functional_category,
            tags_array,
            e.ai_confidence,
            e.needs_review,
            e.name,
            if e.description.is_empty() {
                "暂无".to_string()
            } else {
                e.description.clone()
            },
            e.latest_version,
            e.homepage_url,
            e.download_url,
            e.license,
            e.ai_provider,
        ));
    }
    md
}

// ────────────────── Headless 包装 ──────────────────

pub fn get_catalog_headless(db: &CatalogDB, page: i32, page_size: i32, search: Option<String>) -> Result<Value, String> {
    let search = search.unwrap_or_default();
    let (entries, total) = db
        .get_catalog_entries(&search, page, page_size)
        .map_err(|e| format!("查询目录条目失败: {}", e))?;
    serde_json::to_value(PaginatedCatalog { entries, total })
        .map_err(|e| format!("序列化失败: {}", e))
}

pub fn update_catalog_entry_headless(db: &CatalogDB, body: Value) -> Result<Value, String> {
    let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let mut entry = db
        .get_catalog_entry_by_id(&id)
        .map_err(|e| format!("查询目录条目失败: {}", e))?
        .ok_or_else(|| format!("目录条目不存在: {}", id))?;

    if let Some(v) = body.get("description").and_then(|v| v.as_str()) { entry.description = v.to_string(); }
    if let Some(v) = body.get("homepageUrl").and_then(|v| v.as_str()) { entry.homepage_url = v.to_string(); }
    if let Some(v) = body.get("downloadUrl").and_then(|v| v.as_str()) { entry.download_url = v.to_string(); }
    if let Some(v) = body.get("latestVersion").and_then(|v| v.as_str()) { entry.latest_version = v.to_string(); }
    if let Some(v) = body.get("license").and_then(|v| v.as_str()) { entry.license = v.to_string(); }
    if let Some(v) = body.get("functionalCategory").and_then(|v| v.as_str()) { entry.functional_category = v.to_string(); }
    if let Some(v) = body.get("tags").and_then(|v| v.as_array()) {
        entry.tags = v.iter().filter_map(|t| t.as_str().map(String::from)).collect();
    }
    if let Some(v) = body.get("notes").and_then(|v| v.as_str()) { entry.notes = v.to_string(); }
    if let Some(v) = body.get("needsReview").and_then(|v| v.as_bool()) { entry.needs_review = v; }

    db.update_catalog_entry(&entry)
        .map_err(|e| format!("更新目录条目失败: {}", e))?;
    Ok(Value::Null)
}

pub fn delete_catalog_entry_headless(db: &CatalogDB, id: String) -> Result<Value, String> {
    db.delete_catalog_entry(&id)
        .map_err(|e| format!("删除目录条目失败: {}", e))?;
    Ok(Value::Null)
}

pub fn export_catalog_headless(db: &CatalogDB, format: &str) -> Result<Value, String> {
    let (entries, _total) = db
        .get_catalog_entries("", 1, 1_000_000)
        .map_err(|e| format!("查询目录条目失败: {}", e))?;

    let output = match format.to_lowercase().as_str() {
        "csv" => export_csv(&entries),
        "obsidian" | "markdown" | "md" | "obsidian-md" => export_obsidian_md(&entries),
        _ => return Err(format!("不支持的导出格式: {}", format)),
    };

    Ok(serde_json::json!({"content": output, "format": format}))
}
