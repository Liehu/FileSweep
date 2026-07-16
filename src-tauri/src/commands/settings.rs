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
        obj.insert("migrate_root_dir".to_string(), json!(cfg.migrate_root_dir));
        obj.insert("enable_func_classify".to_string(), json!(cfg.enable_func_classify));
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

    // 迁移根目录
    if let Some(v) = body.get("migrate_root_dir").and_then(|v| v.as_str()) {
        cfg.migrate_root_dir = v.to_string();
    }
    // 功能分类开关
    if let Some(v) = body.get("enable_func_classify").and_then(|v| v.as_bool()) {
        cfg.enable_func_classify = v;
    }
    // GitHub 搜索增强开关 + token
    if let Some(v) = body.get("enable_github_search").and_then(|v| v.as_bool()) {
        cfg.enable_github_search = v;
    }
    if let Some(v) = body.get("github_token").and_then(|v| v.as_str()) {
        cfg.github_token = v.to_string();
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
        obj.insert("migrate_root_dir".to_string(), json!(cfg.migrate_root_dir));
        obj.insert("enable_func_classify".to_string(), json!(cfg.enable_func_classify));
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

    // 迁移根目录
    if let Some(v) = body.get("migrate_root_dir").and_then(|v| v.as_str()) {
        cfg.migrate_root_dir = v.to_string();
    }
    // 功能分类开关
    if let Some(v) = body.get("enable_func_classify").and_then(|v| v.as_bool()) {
        cfg.enable_func_classify = v;
    }
    // GitHub 搜索增强开关 + token
    if let Some(v) = body.get("enable_github_search").and_then(|v| v.as_bool()) {
        cfg.enable_github_search = v;
    }
    if let Some(v) = body.get("github_token").and_then(|v| v.as_str()) {
        cfg.github_token = v.to_string();
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

// ────────────────── AI 配置测试 ──────────────────

/// 测试 AI 提供方的连通性 + 认证 + 模型有效性。
///
/// 用 args 里前端传来的 provider/url/key/model（当前表单值，不必先保存），
/// 发一个极简 ping 请求，快速验证配置是否可用。
///
/// 成功：`{ "ok": true, "model": "...", "latency_ms": 1234 }`
/// 失败：`{ "ok": false, "error": "HTTP 401: {\"error\":{...}}" }`（服务端原始报错透传前端）
pub async fn test_ai_connection(args: Value) -> Result<Value, String> {
    let provider = args
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("custom");

    match provider {
        "openai" | "custom" => {
            // OpenAI 兼容（OpenRouter 走 custom）
            let (key, url, model) = if provider == "custom" {
                let key = args
                    .get("api_key")
                    .or_else(|| args.get("custom_api_key"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let url = args
                    .get("base_url")
                    .or_else(|| args.get("custom_base_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("https://openrouter.ai/api/v1");
                let model = args
                    .get("model")
                    .or_else(|| args.get("custom_model"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                (key, url, model)
            } else {
                let key = args.get("api_key").and_then(|v| v.as_str()).unwrap_or("");
                let url = args
                    .get("base_url")
                    .and_then(|v| v.as_str())
                    .unwrap_or("https://api.openai.com/v1");
                let model = args.get("model").and_then(|v| v.as_str()).unwrap_or("gpt-4o");
                (key, url, model)
            };

            if key.is_empty() {
                return Ok(json!({ "ok": false, "error": "API Key 为空" }));
            }
            if model.is_empty() {
                return Ok(json!({ "ok": false, "error": "模型名称为空" }));
            }

            let enricher = crate::ai::openai::OpenAIEnricher::new(key, url).with_model(model);
            match enricher.test_connection().await {
                Ok(info) => {
                    // info 格式 "model|123ms"
                    let parts: Vec<&str> = info.splitn(2, '|').collect();
                    let m = parts.first().copied().unwrap_or("");
                    let latency = parts
                        .get(1)
                        .and_then(|s| s.strip_suffix("ms").and_then(|n| n.parse::<u64>().ok()))
                        .unwrap_or(0);
                    Ok(json!({ "ok": true, "model": m, "latency_ms": latency }))
                }
                Err(e) => Ok(json!({ "ok": false, "error": e })),
            }
        }
        "ollama" => {
            let url = args
                .get("base_url")
                .or_else(|| args.get("ollama_url"))
                .and_then(|v| v.as_str())
                .unwrap_or("http://localhost:11434");
            // Ollama 测试：GET {url}/api/tags，能连通即认证通过（Ollama 无需 key）
            let client = reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .map_err(|e| format!("HTTP 客户端构建失败: {}", e))?;
            let tags_url = format!("{}/api/tags", url.trim_end_matches('/'));
            let start = std::time::Instant::now();
            let resp = client
                .get(&tags_url)
                .send()
                .await
                .map_err(|e| format!("无法连接 Ollama ({}): {}", url, e))?;
            let latency_ms = start.elapsed().as_millis();
            if resp.status().is_success() {
                Ok(json!({ "ok": true, "model": "ollama", "latency_ms": latency_ms }))
            } else {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                Ok(json!({ "ok": false, "error": format!("HTTP {}: {}", status, body) }))
            }
        }
        "claude" => {
            // Claude 走 OpenAI 兼容封装测试（多数 Claude 代理兼容 OpenAI 格式）
            let key = args
                .get("api_key")
                .or_else(|| args.get("claude_api_key"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let url = args
                .get("base_url")
                .or_else(|| args.get("claude_base_url"))
                .and_then(|v| v.as_str())
                .unwrap_or("https://api.anthropic.com");
            if key.is_empty() {
                return Ok(json!({ "ok": false, "error": "API Key 为空" }));
            }
            let enricher = crate::ai::openai::OpenAIEnricher::new(key, url)
                .with_model(args.get("model").and_then(|v| v.as_str()).unwrap_or("claude-3-sonnet"));
            match enricher.test_connection().await {
                Ok(info) => {
                    let parts: Vec<&str> = info.splitn(2, '|').collect();
                    let m = parts.first().copied().unwrap_or("");
                    let latency = parts
                        .get(1)
                        .and_then(|s| s.strip_suffix("ms").and_then(|n| n.parse::<u64>().ok()))
                        .unwrap_or(0);
                    Ok(json!({ "ok": true, "model": m, "latency_ms": latency }))
                }
                Err(e) => Ok(json!({ "ok": false, "error": e })),
            }
        }
        "offline" => Ok(json!({ "ok": true, "model": "offline", "latency_ms": 0 })),
        other => Ok(json!({ "ok": false, "error": format!("未知 provider: {}", other) })),
    }
}
