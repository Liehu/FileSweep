use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub category: String,
    #[serde(rename = "localPath")]
    pub local_path: String,
    #[serde(rename = "fileSize")]
    pub file_size: i64,
    #[serde(rename = "fileHash")]
    pub file_hash: String,
    pub extension: String,
    #[serde(rename = "functionalCategory")]
    pub functional_category: String,
    pub status: String,
    #[serde(rename = "aiSkip")]
    pub ai_skip: bool,
    #[serde(rename = "scannedAt")]
    pub scanned_at: DateTime<Utc>,
    #[serde(rename = "modTime")]
    pub mod_time: DateTime<Utc>,
    #[serde(rename = "catalogId")]
    pub catalog_id: String,
    #[serde(rename = "isAppDir")]
    pub is_app_dir: bool,
    #[serde(rename = "appDirPath")]
    pub app_dir_path: String,
    #[serde(rename = "appDirReason")]
    pub app_dir_reason: String,
    #[serde(default, rename = "action")]
    pub action: String,
    #[serde(default, rename = "moveTarget")]
    pub move_target: String,
}

impl FileRecord {
    pub fn new_id(hash: &str, path: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(hash.as_bytes());
        hasher.update(path.as_bytes());
        let result = hasher.finalize();
        format!("rec_{}", hex::encode(&result[..8]))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "homepageUrl")]
    pub homepage_url: String,
    #[serde(rename = "downloadUrl")]
    pub download_url: String,
    #[serde(rename = "latestVersion")]
    pub latest_version: String,
    pub license: String,
    #[serde(rename = "functionalCategory")]
    pub functional_category: String,
    pub tags: Vec<String>,
    #[serde(rename = "aiConfidence")]
    pub ai_confidence: f64,
    #[serde(rename = "aiProvider")]
    pub ai_provider: String,
    #[serde(rename = "metaUpdatedAt")]
    pub meta_updated_at: DateTime<Utc>,
    pub notes: String,
    #[serde(rename = "needsReview")]
    pub needs_review: bool,
    #[serde(rename = "aiSkip")]
    pub ai_skip: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationLog {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub operation: String,
    pub source_path: String,
    pub dest_path: String,
    pub reason: String,
    pub file_hash: String,
    pub file_size: i64,
    pub status: String,
    pub session_id: String,
    pub can_revert: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanProgress {
    pub total: usize,
    pub done: usize,
    #[serde(rename = "currentFile")]
    pub current_file: String,
    pub stage: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichProgress {
    pub total: usize,
    pub done: usize,
    #[serde(rename = "needsReview")]
    pub needs_review: usize,
    #[serde(rename = "currentName")]
    pub current_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassifyResult {
    pub category: String,
    #[serde(rename = "target_dir")]
    pub target_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DedupGroup {
    pub representative: FileRecord,
    pub duplicates: Vec<FileRecord>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileStats {
    pub total: i64,
    #[serde(rename = "totalSize")]
    pub total_size: i64,
    pub duplicates: i64,
    pub multiversion: i64,
    pub uncategorized: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichRequest {
    pub name: String,
    pub version: String,
    pub extension: String,
    pub category: String,
    #[serde(rename = "file_size")]
    pub file_size: i64,
    #[serde(rename = "available_tags", skip_serializing_if = "Option::is_none")]
    pub available_tags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichResult {
    pub description: String,
    #[serde(rename = "homepage_url")]
    pub homepage_url: String,
    #[serde(rename = "download_url")]
    pub download_url: String,
    #[serde(rename = "latest_version")]
    pub latest_version: String,
    pub license: String,
    #[serde(rename = "functional_category")]
    pub functional_category: String,
    pub tags: Vec<String>,
    pub confidence: f64,
    #[serde(rename = "needs_review")]
    pub needs_review: bool,
    pub provider: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Operation {
    #[serde(rename = "MOVE")]
    Move,
    #[serde(rename = "DELETE")]
    Delete,
    #[serde(rename = "RENAME")]
    Rename,
}

#[derive(Debug, Clone)]
pub struct ExecutorAction {
    pub operation: Operation,
    pub source: String,
    pub dest: String,
    pub reason: String,
    pub file: FileRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppDirSignature {
    #[serde(rename = "isAppDir")]
    pub is_app_dir: bool,
    #[serde(rename = "mainExe")]
    pub main_exe: String,
    #[serde(rename = "appName")]
    pub app_name: String,
    pub confidence: f64,
    pub reason: String,
}

impl Default for AppDirSignature {
    fn default() -> Self {
        Self {
            is_app_dir: false,
            main_exe: String::new(),
            app_name: String::new(),
            confidence: 0.0,
            reason: String::new(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CategoryRule {
    pub name: String,
    pub target_path: String,
    pub extensions: Vec<String>,
    pub name_keywords: Vec<String>,
    pub app_dir_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesConfig {
    pub categories: Vec<CategoryRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagEntry {
    pub id: String,
    pub name: String,
    pub color: String,
    pub description: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuncCategory {
    pub id: String,
    pub name: String,
    #[serde(rename = "parent_id")]
    pub parent_id: String,
    #[serde(rename = "target_path")]
    pub target_path: String,
    pub extensions: Vec<String>,
    #[serde(rename = "name_keywords")]
    pub name_keywords: Vec<String>,
    #[serde(rename = "sort_order")]
    pub sort_order: i32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AiSettings {
    pub provider: String,
    pub api_key: String,
    pub base_url: String,
    pub model: String,
    pub ollama_url: String,
    pub ollama_model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub scan_dir: String,
    pub recursive: bool,
    pub ai_provider: String,
    pub ai_api_key: String,
    pub ai_base_url: String,
    pub ai_concurrency: i32,
    pub db_path: String,
    pub rules_path: String,
    pub privacy_rules: Vec<String>,
    pub port: i32,
    pub host: String,
    pub log_level: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub openai_key: String,
    pub openai_base_url: String,
    pub claude_key: String,
    pub claude_base_url: String,
    pub custom_ai_name: String,
    pub custom_ai_url: String,
    pub custom_ai_key: String,
    pub custom_ai_model: String,
    pub rules: RulesSettings,
    pub privacy: PrivacySettings,
    pub ai: AiSettings,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RulesSettings {
    pub auto_categorize: bool,
    pub auto_duplicate: bool,
    pub keep_newest_version: bool,
    pub delete_empty_dirs: bool,
    pub move_to_recycle_bin: bool,
    pub min_file_size: i32,
    pub max_file_size: i32,
    pub ignore_patterns: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PrivacySettings {
    pub share_hashes: bool,
    pub share_metadata: bool,
    pub analytics_enabled: bool,
    pub log_retention_days: i32,
}
