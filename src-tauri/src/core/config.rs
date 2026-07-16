pub use crate::core::models::{AiSettings, Config, PrivacySettings, RulesSettings};
use std::fs;
use std::path::{Path, PathBuf};

pub fn load_config(path: &str) -> Result<Config, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("读取配置文件失败: {}", e))?;
    let mut cfg: Config = serde_yaml::from_str(&data).map_err(|e| format!("解析配置文件失败: {}", e))?;
    apply_defaults(&mut cfg);
    Ok(cfg)
}

pub fn save_config(path: &str, config: &Config) -> Result<(), String> {
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建配置目录失败: {}", e))?;
    }
    let data = serde_yaml::to_string(config).map_err(|e| format!("序列化配置失败: {}", e))?;
    fs::write(path, data).map_err(|e| format!("写入配置文件失败: {}", e))
}

pub fn default_config() -> Config {
    let db_path = default_config_path()
        .parent()
        .unwrap_or(Path::new("."))
        .join("catalog.db")
        .to_string_lossy()
        .to_string();

    let rules_path = default_config_path()
        .parent()
        .unwrap_or(Path::new("."))
        .join("rules.yaml")
        .to_string_lossy()
        .to_string();

    Config {
        scan_dir: String::new(),
        recursive: false,
        ai_provider: "offline".to_string(),
        ai_api_key: String::new(),
        ai_base_url: String::new(),
        ai_concurrency: 5,
        ai_batch_size: 20,
        db_path,
        rules_path,
        privacy_rules: Vec::new(),
        port: 8081,
        host: "0.0.0.0".to_string(),
        log_level: "info".to_string(),
        ollama_url: "http://localhost:11434".to_string(),
        ollama_model: String::new(),
        openai_key: String::new(),
        openai_base_url: String::new(),
        claude_key: String::new(),
        claude_base_url: String::new(),
        custom_ai_name: String::new(),
        custom_ai_url: String::new(),
        custom_ai_key: String::new(),
        custom_ai_model: String::new(),
        rules: RulesSettings {
            auto_categorize: true,
            auto_duplicate: true,
            keep_newest_version: true,
            delete_empty_dirs: false,
            move_to_recycle_bin: true,
            min_file_size: 0,
            max_file_size: 0,
            ignore_patterns: String::new(),
        },
        privacy: PrivacySettings {
            share_hashes: false,
            share_metadata: false,
            analytics_enabled: false,
            log_retention_days: 30,
        },
        ai: AiSettings {
            provider: "offline".to_string(),
            api_key: String::new(),
            base_url: String::new(),
            model: String::new(),
            ollama_url: "http://localhost:11434".to_string(),
            ollama_model: String::new(),
        },
        migrate_root_dir: String::new(),
        enable_func_classify: false,
        enable_github_search: true,
        github_token: String::new(),
    }
}

pub fn default_config_path() -> PathBuf {
    if let Some(data_dir) = dirs::data_dir() {
        data_dir.join("FileSweep").join("config").join("config.yaml")
    } else {
        PathBuf::from("config").join("config.yaml")
    }
}

fn apply_defaults(cfg: &mut Config) {
    let def = default_config();
    if cfg.ai_provider.is_empty() {
        cfg.ai_provider = def.ai_provider;
    }
    if cfg.ai_concurrency == 0 {
        cfg.ai_concurrency = def.ai_concurrency;
    }
    if cfg.ai_batch_size == 0 {
        cfg.ai_batch_size = def.ai_batch_size;
    }
    if cfg.db_path.is_empty() {
        cfg.db_path = def.db_path;
    }
    if cfg.rules_path.is_empty() {
        cfg.rules_path = def.rules_path;
    }
    if cfg.port == 0 {
        cfg.port = def.port;
    }
    if cfg.host.is_empty() {
        cfg.host = def.host;
    }
    if cfg.log_level.is_empty() {
        cfg.log_level = def.log_level;
    }
    if cfg.ollama_url.is_empty() {
        cfg.ollama_url = def.ollama_url;
    }
}

impl Config {
    pub fn default_path() -> String {
        default_config_path().to_string_lossy().to_string()
    }

    pub fn categories_path(&self) -> String {
        let config_dir = Path::new(&self.db_path).parent().unwrap_or(Path::new("config"));
        config_dir.join("categories.yaml").to_string_lossy().to_string()
    }

    pub fn offline_db_path(&self) -> String {
        let config_dir = Path::new(&self.db_path).parent().unwrap_or(Path::new("config"));
        config_dir.join("offline_knowledge.db").to_string_lossy().to_string()
    }

    pub fn save(&self, path: &str) -> Result<(), String> {
        save_config(path, self)
    }
}
