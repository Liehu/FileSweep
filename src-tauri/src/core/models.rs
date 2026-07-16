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
    #[serde(default, rename = "appExecutables")]
    pub app_executables: Vec<String>,
    /// 扫描任务 ID（关联 scan_tasks 表），空串表示无关联任务
    #[serde(default, rename = "taskId")]
    pub task_id: String,
}

/// 扫描任务记录（scan_tasks 表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanTask {
    pub id: String,
    #[serde(rename = "scanDir")]
    pub scan_dir: String,
    #[serde(rename = "startedAt")]
    pub started_at: String,
    #[serde(rename = "finishedAt")]
    pub finished_at: String,
    #[serde(rename = "fileCount")]
    pub file_count: i64,
    pub status: String,
    pub recursive: bool,
}

impl FileRecord {
    pub fn new_id(hash: &str, path: &str) -> String {
        let mut hasher = blake3::Hasher::new();
        hasher.update(hash.as_bytes());
        hasher.update(path.as_bytes());
        let result = hasher.finalize();
        // 取前 8 字节十六进制作为短 id
        format!("rec_{}", hex::encode(&result.as_bytes()[..8]))
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
    /// 下载可靠性：high（官方/知名站点，可安全删除后重下）/ medium / low（来源不明，谨慎删除）。
    /// 由 AI 补全，空串表示未评估（回退到内置知名度表判断）。
    #[serde(default, rename = "downloadReliability")]
    pub download_reliability: String,
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
    /// 友好的阶段标签（中文），前端可直接展示
    #[serde(default, rename = "stageLabel")]
    pub stage_label: String,
    /// 阶段是否为不确定进度（前端应展示动画而非百分比）
    #[serde(default, rename = "indeterminate")]
    pub indeterminate: bool,
    /// 本阶段处理速率（项/秒），仅 hashing 等阶段有意义
    #[serde(default, rename = "ratePerSec")]
    pub rate_per_sec: f64,
    /// 预计剩余秒数，<= 0 表示未知
    #[serde(default, rename = "etaSec")]
    pub eta_sec: i64,
}

impl ScanProgress {
    /// 构造一个不确定进度的阶段事件（如 walking）
    pub fn indeterminate(stage: &str, stage_label: &str, done: usize, current_file: impl Into<String>) -> Self {
        Self {
            total: 0,
            done,
            current_file: current_file.into(),
            stage: stage.into(),
            stage_label: stage_label.into(),
            indeterminate: true,
            rate_per_sec: 0.0,
            eta_sec: 0,
        }
    }

    /// 构造一个确定进度的阶段事件（如 hashing）
    pub fn determinate(stage: &str, stage_label: &str, total: usize, done: usize, current_file: impl Into<String>, rate_per_sec: f64, eta_sec: i64) -> Self {
        Self {
            total,
            done,
            current_file: current_file.into(),
            stage: stage.into(),
            stage_label: stage_label.into(),
            indeterminate: false,
            rate_per_sec,
            eta_sec,
        }
    }
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
    /// 功能/行业分类（func_categories 关键词匹配结果，可能为空）
    #[serde(default, rename = "functional_category")]
    pub functional_category: Option<String>,
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
    /// GitHub 搜索命中（作为"已知事实"提示给 LLM，提升功能分类准确性）。
    /// 由 enrich 流程在构建请求时搜 GitHub 填充；无命中时为 None。
    #[serde(default, skip)]
    pub github_hint: Option<crate::ai::github_search::GitHubHint>,
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
    /// 下载可靠性 high/medium/low，由 LLM 判断，空串表示未评估。
    #[serde(default, rename = "download_reliability")]
    pub download_reliability: String,
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
    /// AI 批量补全的批大小（每批文件数，默认 20）。
    /// OpenRouter free 模型输出上限 ~4096 token，批大小 20 时输出 ~3K token 留余量防截断。
    #[serde(default = "default_ai_batch_size")]
    pub ai_batch_size: i32,
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
    /// 迁移根目录：target_path 为相对路径时拼接此根目录；空则相对路径当工作目录
    #[serde(default)]
    pub migrate_root_dir: String,
    /// 扫描时是否启用功能分类关键词匹配（func_categories）
    #[serde(default)]
    pub enable_func_classify: bool,
    /// GitHub 搜索增强：enrich 前先搜 GitHub 拿仓库事实，提升 AI 功能分类准确性。
    #[serde(default = "default_true")]
    pub enable_github_search: bool,
    /// GitHub Personal Access Token（可选，fine-grained 只读 public 即可）。
    /// 填了走认证 30 req/min，不填走未认证 10 req/min。
    #[serde(default)]
    pub github_token: String,
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

/// AI 批量补全的默认批大小：20。
/// OpenRouter free 模型输出上限 ~4096 token，批大小 20 时输出 ~3K token 留余量防截断。
fn default_ai_batch_size() -> i32 {
    20
}

/// 默认 true（用于 #[serde(default = "default_true")] 的 bool 字段）。
fn default_true() -> bool {
    true
}
