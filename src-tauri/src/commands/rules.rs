use std::sync::Arc;
use tauri::State;

use crate::core::classifier::{default_rules, load_rules_from_yaml, save_rules_to_yaml, RulesConfig};
use crate::core::config::Config;

use serde_json::Value;

/// 从 config 目录加载分类规则配置（rules.yaml）。
/// 文件不存在时返回默认规则。
#[tauri::command]
pub async fn get_rules(
    config: State<'_, Arc<Config>>,
) -> Result<RulesConfig, String> {
    let path = config.rules_path.clone();
    match load_rules_from_yaml(&path) {
        Ok(rules) => Ok(rules),
        Err(_) => Ok(default_rules()),
    }
}

/// 更新分类规则配置并保存到 rules.yaml。
#[tauri::command]
pub async fn update_rules(
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    rules: RulesConfig,
) -> Result<(), String> {
    let path = config.inner().read().rules_path.clone();
    save_rules_to_yaml(&path, &rules)
        .map_err(|e| format!("保存分类规则失败: {}", e))
}

// ────────────────── Headless Wrappers ──────────────────

pub async fn get_rules_headless(config: &Arc<tokio::sync::RwLock<Config>>) -> Result<Value, String> {
    let cfg = config.read().await;
    let path = cfg.rules_path.clone();
    let rules = match load_rules_from_yaml(&path) {
        Ok(r) => r,
        Err(_) => default_rules(),
    };
    serde_json::to_value(rules).map_err(|e| format!("序列化规则失败: {}", e))
}

pub async fn update_rules_headless(config: &Arc<tokio::sync::RwLock<Config>>, body: Value) -> Result<Value, String> {
    let rules: RulesConfig = serde_json::from_value(body)
        .map_err(|e| format!("解析分类规则失败: {}", e))?;
    let cfg = config.read().await;
    let path = cfg.rules_path.clone();
    save_rules_to_yaml(&path, &rules)
        .map_err(|e| format!("保存分类规则失败: {}", e))?;
    serde_json::to_value(rules).map_err(|e| format!("序列化规则失败: {}", e))
}
