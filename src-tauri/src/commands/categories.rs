use std::sync::Arc;
use tauri::State;

use crate::core::classifier::FuncCategory;
use crate::core::config::Config;

use serde_json::Value;

/// 从 config 目录加载功能分类列表（categories.yaml）。
#[tauri::command]
pub async fn get_func_categories(
    config: State<'_, Arc<Config>>,
) -> Result<Vec<FuncCategory>, String> {
    let path = config.categories_path();
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return Ok(Vec::new()),
    };

    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&data).map_err(|e| format!("解析分类文件失败: {}", e))?;

    let categories = parsed
        .get("categories")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| {
                    Some(FuncCategory {
                        name: item.get("name")?.as_str()?.to_string(),
                        description: item
                            .get("description")
                            .and_then(|d| d.as_str())
                            .map(String::from),
                        parent: item
                            .get("parent")
                            .and_then(|p| p.as_str())
                            .map(String::from),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    Ok(categories)
}

/// 更新功能分类列表并保存到 categories.yaml。
#[tauri::command]
pub async fn update_func_categories(
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    categories: Vec<FuncCategory>,
) -> Result<(), String> {
    let path = config.inner().read().categories_path();

    // 序列化为 YAML 结构
    let items: Vec<serde_yaml::Value> = categories
        .iter()
        .map(|c| {
            let mut map = serde_yaml::Mapping::new();
            map.insert(
                serde_yaml::Value::String("name".into()),
                serde_yaml::Value::String(c.name.clone()),
            );
            if let Some(ref desc) = c.description {
                map.insert(
                    serde_yaml::Value::String("description".into()),
                    serde_yaml::Value::String(desc.clone()),
                );
            }
            if let Some(ref parent) = c.parent {
                map.insert(
                    serde_yaml::Value::String("parent".into()),
                    serde_yaml::Value::String(parent.clone()),
                );
            }
            serde_yaml::Value::Mapping(map)
        })
        .collect();

    let mut root = serde_yaml::Mapping::new();
    root.insert(
        serde_yaml::Value::String("categories".into()),
        serde_yaml::Value::Sequence(items),
    );

    let yaml_str = serde_yaml::to_string(&root)
        .map_err(|e| format!("序列化分类失败: {}", e))?;

    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建分类目录失败: {}", e))?;
    }
    std::fs::write(&path, yaml_str)
        .map_err(|e| format!("写入分类文件失败: {}", e))?;

    Ok(())
}

// ────────────────── Headless 包装 ──────────────────

pub async fn get_func_categories_headless(config: &Arc<tokio::sync::RwLock<Config>>) -> Result<Value, String> {
    let cfg = config.read().await;
    let path = cfg.categories_path();
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => return Ok(serde_json::json!([])),
    };

    let parsed: serde_yaml::Value =
        serde_yaml::from_str(&data).map_err(|e| format!("解析分类文件失败: {}", e))?;

    let categories = parsed
        .get("categories")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|item| {
                    Some(FuncCategory {
                        name: item.get("name")?.as_str()?.to_string(),
                        description: item.get("description").and_then(|d| d.as_str()).map(String::from),
                        parent: item.get("parent").and_then(|p| p.as_str()).map(String::from),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    serde_json::to_value(categories).map_err(|e| format!("序列化失败: {}", e))
}

pub async fn update_func_categories_headless(config: &Arc<tokio::sync::RwLock<Config>>, body: Value) -> Result<Value, String> {
    let categories: Vec<FuncCategory> = serde_json::from_value(
        body.get("categories").cloned().unwrap_or(serde_json::json!([]))
    ).map_err(|e| format!("解析参数失败: {}", e))?;

    let cfg = config.read().await;
    let path = cfg.categories_path();

    let items: Vec<serde_yaml::Value> = categories
        .iter()
        .map(|c| {
            let mut map = serde_yaml::Mapping::new();
            map.insert(serde_yaml::Value::String("name".into()), serde_yaml::Value::String(c.name.clone()));
            if let Some(ref desc) = c.description {
                map.insert(serde_yaml::Value::String("description".into()), serde_yaml::Value::String(desc.clone()));
            }
            if let Some(ref parent) = c.parent {
                map.insert(serde_yaml::Value::String("parent".into()), serde_yaml::Value::String(parent.clone()));
            }
            serde_yaml::Value::Mapping(map)
        })
        .collect();

    let mut root = serde_yaml::Mapping::new();
    root.insert(serde_yaml::Value::String("categories".into()), serde_yaml::Value::Sequence(items));

    let yaml_str = serde_yaml::to_string(&root).map_err(|e| format!("序列化分类失败: {}", e))?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("创建分类目录失败: {}", e))?;
    }
    std::fs::write(&path, yaml_str).map_err(|e| format!("写入分类文件失败: {}", e))?;

    Ok(Value::Null)
}
