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

    /// 类型分类：纯扩展名匹配（不再用关键词）。
    ///
    /// 规则按 priority DESC 排序，第一个 extensions 命中的规则胜出。
    /// app_dir_only 规则只匹配 app dir，非 app_dir_only 规则跳过 app dir。
    pub fn classify(&self, file: &FileRecord) -> ClassifyResult {
        let rules = self.sorted_rules();
        let ext_lower = file.extension.to_lowercase();

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
                    functional_category: None,
                };
            }
            // 纯扩展名匹配（不再读 name_keywords）
            if match_extension(&rule.extensions, &ext_lower) {
                return ClassifyResult {
                    category: rule.name.clone(),
                    target_dir: rule.target_path.clone(),
                    functional_category: None,
                };
            }
        }

        ClassifyResult {
            category: "未分类".to_string(),
            target_dir: "Uncategorized".to_string(),
            functional_category: None,
        }
    }

    /// 功能/行业分类：用 func_categories 的 keywords 做 token 精确匹配。
    ///
    /// 将文件名+路径按分隔符拆成 token，关键词必须精确等于某个 token（非子串匹配）。
    /// 返回第一个命中的 func_category 名称。
    pub fn classify_functional(
        file: &FileRecord,
        func_categories: &[crate::db::config::FuncCategoryRow],
    ) -> Option<String> {
        if func_categories.is_empty() {
            return None;
        }
        // 拼接文件名 + 路径用于匹配（路径含目录名，可能有关键词）
        let combined = format!("{} {}", file.name, file.local_path);
        let tokens = split_tokens(&combined);

        for fc in func_categories {
            if !fc.enabled {
                continue;
            }
            // 任一 keyword 命中任意 token 即归类
            if fc.keywords.iter().any(|kw| {
                let kw_lower = kw.to_lowercase();
                tokens.iter().any(|t| t == &kw_lower)
            }) {
                return Some(fc.name.clone());
            }
        }
        None
    }

    fn sorted_rules(&self) -> &Vec<CategoryRule> {
        self.sorted_cache.get_or_init(|| {
            let mut rules = self.rules.categories.clone();
            // 预计算 lowercase 的 extensions（避免重复 to_lowercase）
            for r in &mut rules {
                r.extensions = r.extensions.iter().map(|e| e.to_lowercase()).collect();
            }
            // 按 priority DESC 排序（同 priority 保持原序）
            rules.sort_by(|a, b| {
                let pa = priority_of(a);
                let pb = priority_of(b);
                pb.cmp(&pa)
            });
            rules
        })
    }
}

/// 从规则名提取 priority（兼容旧式 name 不含 priority 的情况）。
/// 当前 CategoryRule 无 priority 字段，统一返回 0（保持原序）。
/// 后续若 DB 表加 priority 列，可改为读取。
fn priority_of(_rule: &CategoryRule) -> i32 {
    0
}

/// 将字符串按分隔符拆分为小写 token。
///
/// 分隔符：`-` `_` `.` `/` `\` 空格。
/// 驼峰命名（无分隔符）不拆分，整体作为一个 token。
/// 例："pre-setup-wsl.sh" → ["pre", "setup", "wsl", "sh"]
///     "WechatOpenSetUpController" → ["wechatopensetupcontroller"]
fn split_tokens(s: &str) -> Vec<String> {
    s.split(|c: char| matches!(c, '-' | '_' | '.' | '/' | '\\' | ' '))
        .filter(|t| !t.is_empty())
        .map(|t| t.to_lowercase())
        .collect()
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
                app_dir_only: false,
            },
            CategoryRule {
                name: "文档".to_string(),
                target_path: "Docs".to_string(),
                extensions: vec![".pdf", ".docx", ".doc", ".xls", ".xlsx", ".ppt", ".pptx", ".md", ".txt", ".epub"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "压缩包".to_string(),
                target_path: "Archives".to_string(),
                extensions: vec![".zip", ".7z", ".rar", ".gz", ".tar", ".xz", ".bz2", ".tar.gz", ".tar.xz", ".tar.bz2"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "脚本".to_string(),
                target_path: "Scripts".to_string(),
                extensions: vec![".sh", ".bash", ".py", ".bat", ".cmd", ".ps1", ".rb", ".pl"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "Java工具".to_string(),
                target_path: "Jars".to_string(),
                extensions: vec![".jar", ".war"].iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "镜像".to_string(),
                target_path: "Images".to_string(),
                extensions: vec![".iso", ".img", ".vmdk", ".vhd"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "视频".to_string(),
                target_path: "Videos".to_string(),
                extensions: vec![".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "音频".to_string(),
                target_path: "Audio".to_string(),
                extensions: vec![".mp3", ".flac", ".wav", ".aac", ".ogg", ".wma"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
            CategoryRule {
                name: "图片".to_string(),
                target_path: "Pictures".to_string(),
                extensions: vec![".png", ".jpg", ".jpeg", ".gif", ".webp", ".svg"]
                    .iter().map(|s| s.to_string()).collect(),
                app_dir_only: false,
            },
        ],
    }
}

fn match_extension(extensions: &[String], ext_lower: &str) -> bool {
    extensions.iter().any(|e| e.eq_ignore_ascii_case(ext_lower))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::models::FileRecord;

    fn make_file(name: &str, ext: &str) -> FileRecord {
        FileRecord {
            name: name.to_string(),
            extension: ext.to_string(),
            ..Default::default()
        }
    }

    /// 验证职责分离后的类型分类：纯扩展名匹配，不再被关键词劫持
    #[test]
    fn test_classify_extension_only() {
        let clf = Classifier::with_defaults();

        // .sh 文件即使文件名含 "setup" 也归脚本（不再被安装包关键词劫持）
        assert_eq!(clf.classify(&make_file("pre-setup-wsl", ".sh")).category, "脚本");
        // .php 无规则 → 未分类（不会被 "update" 关键词劫持到安装包）
        assert_eq!(clf.classify(&make_file("config_update", ".php")).category, "未分类");
        // .java 无规则 → 未分类（驼峰 WechatOpenSetUpController 不拆 token）
        assert_eq!(clf.classify(&make_file("WechatOpenSetUpController", ".java")).category, "未分类");
        // 正常案例
        assert_eq!(clf.classify(&make_file("setup", ".exe")).category, "安装包");
        assert_eq!(clf.classify(&make_file("readme", ".md")).category, "文档");
        assert_eq!(clf.classify(&make_file("archive", ".zip")).category, "压缩包");
    }

    /// 验证 split_tokens 拆分规则
    #[test]
    fn test_split_tokens() {
        assert_eq!(split_tokens("pre-setup-wsl.sh"), vec!["pre", "setup", "wsl", "sh"]);
        // 驼峰不拆
        assert_eq!(split_tokens("WechatOpenSetUpController"), vec!["wechatopensetupcontroller"]);
        // 空字符串
        assert!(split_tokens("").is_empty());
        // 多分隔符
        assert_eq!(split_tokens("app_v2.1/test"), vec!["app", "v2", "1", "test"]);
    }

    /// 验证 classify_functional：func_categories 关键词 token 精确匹配
    #[test]
    fn test_classify_functional() {
        use crate::db::config::FuncCategoryRow;

        let func_cats = vec![
            FuncCategoryRow {
                id: 1,
                name: "Boot".to_string(),
                keywords: vec!["EasyBCD".to_string(), "rEFInd".to_string()],
                parent: "操作系统".to_string(),
                description: String::new(),
                target_path: String::new(),
                enabled: true,
            },
            FuncCategoryRow {
                id: 2,
                name: "Exp-Frameworks".to_string(),
                keywords: vec!["Metasploit".to_string()],
                parent: "网络安全".to_string(),
                description: String::new(),
                target_path: String::new(),
                enabled: true,
            },
        ];

        // 文件名含关键词 token
        let f1 = make_file("EasyBCD-2.4.2", ".exe");
        assert_eq!(
            Classifier::classify_functional(&f1, &func_cats),
            Some("Boot".to_string())
        );

        // 路径含关键词（拼接 name + path 匹配）
        let f2 = FileRecord {
            name: "config".to_string(),
            local_path: "D:\\Tools\\Metasploit\\config".to_string(),
            extension: ".cfg".to_string(),
            ..Default::default()
        };
        assert_eq!(
            Classifier::classify_functional(&f2, &func_cats),
            Some("Exp-Frameworks".to_string())
        );

        // 无命中
        let f3 = make_file("random-file", ".txt");
        assert_eq!(Classifier::classify_functional(&f3, &func_cats), None);

        // 子串不匹配（含 "easy" 但不是独立 token）
        let f4 = make_file("uneasy-reader", ".pdf");
        assert_eq!(Classifier::classify_functional(&f4, &func_cats), None);

        // 空列表
        assert_eq!(Classifier::classify_functional(&f1, &[]), None);
    }
}
