use std::sync::Arc;
use tauri::State;

use crate::core::config::{Config, PrivacySettings, RulesSettings};

use serde_json::{json, Value};

/// 读取当前配置（从 config.yaml 文件加载）。
/// 返回与前端期望格式一致的 JSON（ai 字段包含 custom_* 子字段）。
#[tauri::command]
pub async fn get_settings(
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
) -> Result<Value, String> {
    let cfg = config.inner().read().clone();
    let ai = json!({
        "provider": cfg.ai_provider,
        "openai_api_key": cfg.openai_key,
        "openai_base_url": cfg.openai_base_url,
        "openai_model": cfg.ai.model,
        "ollama_url": cfg.ollama_url,
        "ollama_model": cfg.ollama_model,
        "claude_api_key": cfg.claude_key,
        "claude_base_url": cfg.claude_base_url,
        "claude_model": "",
        "custom_name": cfg.custom_ai_name,
        "custom_base_url": cfg.custom_ai_url,
        "custom_api_key": cfg.custom_ai_key,
        "custom_model": cfg.custom_ai_model,
    });
    let privacy = json!({
        "exclude_private": !cfg.privacy.share_metadata,
        "exclude_system": !cfg.privacy.share_hashes,
        "log_retention_days": cfg.privacy.log_retention_days,
    });
    let mut val = serde_json::to_value(&cfg)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert("ai".to_string(), ai);
        obj.insert("privacy".to_string(), privacy);
    }
    Ok(val)
}

/// 部分更新配置项，并持久化到 config.yaml。
///
/// 接收 JSON body，手动映射前端字段到 Config 结构体。
#[tauri::command]
pub async fn update_settings(
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    body: Value,
) -> Result<(), String> {
    let mut cfg = config.inner().read().clone();

    if let Some(r) = body.get("rules") {
        let r: RulesSettings = serde_json::from_value(r.clone())
            .map_err(|e| format!("解析 rules 失败: {}", e))?;
        cfg.rules = r;
    }
    if let Some(p) = body.get("privacy") {
        // 前端发送 exclude_private/exclude_system，映射到后端 share_metadata/share_hashes（取反）
        if let Some(v) = p.get("exclude_private").and_then(|v| v.as_bool()) { cfg.privacy.share_metadata = !v; }
        if let Some(v) = p.get("exclude_system").and_then(|v| v.as_bool()) { cfg.privacy.share_hashes = !v; }
        if let Some(v) = p.get("log_retention_days").and_then(|v| v.as_i64()) { cfg.privacy.log_retention_days = v as i32; }
    }
    if let Some(a) = body.get("ai") {
        // 前端发送 camelCase 字段名，映射到 Config 的扁平字段
        if let Some(v) = a.get("provider").and_then(|v| v.as_str()) { cfg.ai_provider = v.to_string(); }
        // OpenAI
        if let Some(v) = a.get("openai_api_key").and_then(|v| v.as_str()) { cfg.openai_key = v.to_string(); cfg.ai_api_key = v.to_string(); }
        if let Some(v) = a.get("openai_base_url").and_then(|v| v.as_str()) { cfg.openai_base_url = v.to_string(); cfg.ai_base_url = v.to_string(); }
        if let Some(v) = a.get("openai_model").and_then(|v| v.as_str()) { cfg.ai.model = v.to_string(); }
        // Ollama
        if let Some(v) = a.get("ollama_url").and_then(|v| v.as_str()) { cfg.ollama_url = v.to_string(); cfg.ai.ollama_url = v.to_string(); }
        if let Some(v) = a.get("ollama_model").and_then(|v| v.as_str()) { cfg.ollama_model = v.to_string(); cfg.ai.ollama_model = v.to_string(); }
        // Claude
        if let Some(v) = a.get("claude_api_key").and_then(|v| v.as_str()) { cfg.claude_key = v.to_string(); }
        if let Some(v) = a.get("claude_base_url").and_then(|v| v.as_str()) { cfg.claude_base_url = v.to_string(); }
        // Custom
        if let Some(v) = a.get("custom_name").and_then(|v| v.as_str()) { cfg.custom_ai_name = v.to_string(); }
        if let Some(v) = a.get("custom_base_url").and_then(|v| v.as_str()) { cfg.custom_ai_url = v.to_string(); }
        if let Some(v) = a.get("custom_api_key").and_then(|v| v.as_str()) { cfg.custom_ai_key = v.to_string(); }
        if let Some(v) = a.get("custom_model").and_then(|v| v.as_str()) { cfg.custom_ai_model = v.to_string(); }
    }

    let config_path = crate::core::config::default_config_path()
        .to_string_lossy()
        .to_string();
    cfg.save(&config_path)
        .map_err(|e| format!("保存配置失败: {}", e))?;

    *config.inner().write() = cfg;
    log::info!("配置已保存到 {}", config_path);

    Ok(())
}

// ────────────────── Headless Wrappers ──────────────────

pub async fn get_settings_headless(config: &Arc<tokio::sync::RwLock<Config>>) -> Result<Value, String> {
    let cfg = config.read().await;
    // 构建前端期望的嵌套格式
    let ai = json!({
        "provider": cfg.ai_provider,
        "openai_api_key": cfg.openai_key,
        "openai_base_url": cfg.openai_base_url,
        "openai_model": cfg.ai.model,
        "ollama_url": cfg.ollama_url,
        "ollama_model": cfg.ollama_model,
        "claude_api_key": cfg.claude_key,
        "claude_base_url": cfg.claude_base_url,
        "claude_model": "",
        "custom_name": cfg.custom_ai_name,
        "custom_base_url": cfg.custom_ai_url,
        "custom_api_key": cfg.custom_ai_key,
        "custom_model": cfg.custom_ai_model,
    });
    let mut val = serde_json::to_value(&*cfg)
        .map_err(|e| format!("序列化配置失败: {}", e))?;
    if let Some(obj) = val.as_object_mut() {
        obj.insert("ai".to_string(), ai);
    }
    Ok(val)
}

pub async fn update_settings_headless(config: &Arc<tokio::sync::RwLock<Config>>, body: Value) -> Result<Value, String> {
    let mut cfg = config.read().await.clone();

    if let Some(r) = body.get("rules") {
        let r: RulesSettings = serde_json::from_value(r.clone())
            .map_err(|e| format!("解析 rules 失败: {}", e))?;
        cfg.rules = r;
    }
    if let Some(p) = body.get("privacy") {
        let p: PrivacySettings = serde_json::from_value(p.clone())
            .map_err(|e| format!("解析 privacy 失败: {}", e))?;
        cfg.privacy = p;
    }
    if let Some(a) = body.get("ai") {
        // 前端发送 camelCase 字段名，需要映射到 Config 的扁平字段
        if let Some(v) = a.get("provider").and_then(|v| v.as_str()) { cfg.ai_provider = v.to_string(); }
        // OpenAI
        if let Some(v) = a.get("openai_api_key").and_then(|v| v.as_str()) { cfg.openai_key = v.to_string(); cfg.ai_api_key = v.to_string(); }
        if let Some(v) = a.get("openai_base_url").and_then(|v| v.as_str()) { cfg.openai_base_url = v.to_string(); cfg.ai_base_url = v.to_string(); }
        if let Some(v) = a.get("openai_model").and_then(|v| v.as_str()) { /* no top-level field, store in ai */ cfg.ai.model = v.to_string(); }
        // Ollama
        if let Some(v) = a.get("ollama_url").and_then(|v| v.as_str()) { cfg.ollama_url = v.to_string(); cfg.ai.ollama_url = v.to_string(); }
        if let Some(v) = a.get("ollama_model").and_then(|v| v.as_str()) { cfg.ollama_model = v.to_string(); cfg.ai.ollama_model = v.to_string(); }
        // Claude
        if let Some(v) = a.get("claude_api_key").and_then(|v| v.as_str()) { cfg.claude_key = v.to_string(); }
        if let Some(v) = a.get("claude_base_url").and_then(|v| v.as_str()) { cfg.claude_base_url = v.to_string(); }
        if let Some(v) = a.get("claude_model").and_then(|v| v.as_str()) { /* no top-level field */ }
        // Custom
        if let Some(v) = a.get("custom_name").and_then(|v| v.as_str()) { cfg.custom_ai_name = v.to_string(); }
        if let Some(v) = a.get("custom_base_url").and_then(|v| v.as_str()) { cfg.custom_ai_url = v.to_string(); }
        if let Some(v) = a.get("custom_api_key").and_then(|v| v.as_str()) { cfg.custom_ai_key = v.to_string(); }
        if let Some(v) = a.get("custom_model").and_then(|v| v.as_str()) { cfg.custom_ai_model = v.to_string(); }
    }

    let config_path = crate::core::config::default_config_path()
        .to_string_lossy()
        .to_string();
    cfg.save(&config_path)
        .map_err(|e| format!("保存配置失败: {}", e))?;

    log::info!("配置已保存到 {}", config_path);

    *config.write().await = cfg.clone();

    serde_json::to_value(cfg).map_err(|e| format!("序列化配置失败: {}", e))
}
