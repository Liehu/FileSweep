pub use crate::core::models::{CategoryRule, ClassifyResult, FileRecord, RulesConfig};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FuncCategory {
    pub name: String,
    pub description: Option<String>,
    pub parent: Option<String>,
}

use std::fs;
use std::path::Path;

pub struct Classifier {
    pub rules: RulesConfig,
    sorted_cache: std::sync::OnceLock<Vec<CategoryRule>>,
}

impl Classifier {
    pub fn new(rules_path: &str) -> Result<Self, String> {
        let data = fs::read_to_string(rules_path)
            .map_err(|e| format!("读取规则文件失败: {}", e))?;
        let cfg: RulesConfig =
            serde_yaml::from_str(&data).map_err(|e| format!("解析规则文件失败: {}", e))?;
        Ok(Self { rules: cfg, sorted_cache: std::sync::OnceLock::new() })
    }

    pub fn with_rules(rules: RulesConfig) -> Self {
        Self { rules, sorted_cache: std::sync::OnceLock::new() }
    }

    pub fn with_defaults() -> Self {
        Self {
            rules: default_rules(),
            sorted_cache: std::sync::OnceLock::new(),
        }
    }

    pub fn classify(&self, file: &FileRecord) -> ClassifyResult {
        let rules = self.sorted_rules();

        for rule in rules {
            if rule.app_dir_only && !file.is_app_dir {
                continue;
            }
            if file.is_app_dir && !rule.app_dir_only {
                continue;
            }
            if file.is_app_dir && rule.app_dir_only {
                return ClassifyResult {
                    category: rule.name.clone(),
                    target_dir: rule.target_path.clone(),
                };
            }
            if match_extension(&rule.extensions, &file.extension)
                || match_keywords(&rule.name_keywords, &file.name)
            {
                return ClassifyResult {
                    category: rule.name.clone(),
                    target_dir: rule.target_path.clone(),
                };
            }
        }

        ClassifyResult {
            category: "未分类".to_string(),
            target_dir: "Uncategorized".to_string(),
        }
    }

    fn sorted_rules(&self) -> &Vec<CategoryRule> {
        self.sorted_cache.get_or_init(|| {
            let mut rules = self.rules.categories.clone();
            rules.sort_by(|a, b| {
                let da = a.name.matches('\\').count();
                let db = b.name.matches('\\').count();
                db.cmp(&da)
            });
            rules
        })
    }
}

pub fn load_rules_from_yaml(path: &str) -> Result<RulesConfig, String> {
    let data = fs::read_to_string(path).map_err(|e| format!("读取规则文件失败: {}", e))?;
    let cfg: RulesConfig =
        serde_yaml::from_str(&data).map_err(|e| format!("解析规则文件失败: {}", e))?;
    Ok(cfg)
}

pub fn save_rules_to_yaml(path: &str, rules: &RulesConfig) -> Result<(), String> {
    let data = serde_yaml::to_string(rules).map_err(|e| format!("序列化规则失败: {}", e))?;
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }
    fs::write(path, data).map_err(|e| format!("写入规则文件失败: {}", e))
}

pub fn default_rules() -> RulesConfig {
    RulesConfig {
        categories: vec![
            CategoryRule {
                name: "安装包".to_string(),
                target_path: "Installers".to_string(),
                extensions: vec![".exe", ".msi", ".pkg", ".dmg", ".deb", ".rpm", ".AppImage"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec!["setup", "install", "installer", "update"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "文档".to_string(),
                target_path: "Docs".to_string(),
                extensions: vec![".pdf", ".docx", ".doc", ".xls", ".xlsx", ".ppt", ".pptx", ".md", ".txt", ".epub"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "压缩包".to_string(),
                target_path: "Archives".to_string(),
                extensions: vec![".zip", ".7z", ".rar", ".gz", ".tar", ".xz", ".bz2", ".tar.gz", ".tar.xz", ".tar.bz2"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "脚本".to_string(),
                target_path: "Scripts".to_string(),
                extensions: vec![".sh", ".bash", ".py", ".bat", ".cmd", ".ps1", ".rb", ".pl"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "Java工具".to_string(),
                target_path: "Jars".to_string(),
                extensions: vec![".jar", ".war"].iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "镜像".to_string(),
                target_path: "Images".to_string(),
                extensions: vec![".iso", ".img", ".vmdk", ".vhd"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "视频".to_string(),
                target_path: "Videos".to_string(),
                extensions: vec![".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "音频".to_string(),
                target_path: "Audio".to_string(),
                extensions: vec![".mp3", ".flac", ".wav", ".aac", ".ogg", ".wma"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
            CategoryRule {
                name: "图片".to_string(),
                target_path: "Pictures".to_string(),
                extensions: vec![".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg"]
                    .iter().map(|s| s.to_string()).collect(),
                name_keywords: vec![],
                app_dir_only: false,
            },
        ],
    }
}

fn match_extension(extensions: &[String], ext: &str) -> bool {
    let lower = ext.to_lowercase();
    extensions.iter().any(|e| e.to_lowercase() == lower)
}

fn match_keywords(keywords: &[String], name: &str) -> bool {
    let lower = name.to_lowercase();
    keywords.iter().any(|kw| lower.contains(&kw.to_lowercase()))
}

pub fn is_redundant_archive(file: &FileRecord, all_files: &[FileRecord]) -> bool {
    let archive_exts: std::collections::HashSet<&str> =
        [".zip", ".7z", ".rar", ".gz", ".tar"].iter().copied().collect();
    let ext_lower = file.extension.to_lowercase();
    let name_lower = file.name.to_lowercase();
    let is_tar_variant = name_lower.ends_with(".tar.gz")
        || name_lower.ends_with(".tar.bz2")
        || name_lower.ends_with(".tar.xz");

    if !archive_exts.contains(ext_lower.as_str()) && !is_tar_variant {
        return false;
    }

    let normalized = normalize_archive_name(&file.name);
    for other in all_files {
        if other.id == file.id {
            continue;
        }
        let other_ext_lower = other.extension.to_lowercase();
        if archive_exts.contains(other_ext_lower.as_str()) {
            continue;
        }
        if normalize_archive_name(&other.name) == normalized {
            return true;
        }
        if !other.extension.is_empty() {
            let other_base = other.name.strip_suffix(&other.extension).unwrap_or(&other.name);
            if normalize_archive_name(other_base) == normalized {
                return true;
            }
        }
    }
    false
}

fn normalize_archive_name(name: &str) -> String {
    let mut base = name.to_string();
    let ext = std::path::Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    if !ext.is_empty() {
        base = base[..base.len() - ext.len()].to_string();
    }
    let base_lower = base.to_lowercase();
    if base_lower.ends_with(".tar") {
        base = base[..base.len() - 4].to_string();
    }
    let mut base = base.to_lowercase();
    for sep in &["-", "_", ".", " "] {
        base = base.replace(sep, "");
    }
    for suffix in &[
        "setup", "install", "installer", "win64", "win32", "amd64", "x64", "x86", "64bit", "32bit",
    ] {
        if base.ends_with(suffix) {
            base = base[..base.len() - suffix.len()].to_string();
        }
    }
    base
}
