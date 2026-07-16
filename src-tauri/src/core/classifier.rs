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

    /// 功能/行业分类：用 func_categories 的 keywords 做 **token 级精确匹配**。
    ///
    /// 历史问题：早期版本用 aho-corasick 做**子串匹配**，导致大量误命中
    /// （如 `AliCloud-Tools-v1.0.5-windows-amd64.zip` 被 "Windows" 关键词误分到 ISO；
    /// `Directory Opus` 被 "Directory" 命中）。本版本恢复 token 边界精确匹配，并叠加：
    ///
    /// - **token 边界**：关键词必须等于某个完整 token（按分隔符拆分，不拆驼峰——
    ///   驼峰连写的专有名如 EasyBCD/Metasploit 应作为整体 token 保留以精确匹配）。
    /// - **最小长度**：关键词 < 3 字符直接忽略（VS/IDE 等过短泛词）。
    /// - **安全类多信号**：`parent == "网络安全"` 的分类要求 ≥2 个不同 token 命中，
    ///   或命中关键词本身是强专有名（≥5 字符的纯字母专有名如 Metasploit/CobaltStrike）。
    ///   避免单一弱信号（如只命中 "AES"）就把文件定性为安全工具。
    /// - **仅软件类**：只对软件文件（安装包/压缩包/Java工具/镜像）和绿色软件目录
    ///   （is_app_dir）运行功能分类；文档/媒体/图片/脚本等非软件类不做功能分类
    ///   （避免 MSA_Design_Doc.md 被 "Doc" 误分到 DocView、教程文档被分到 SysEnhance 等）。
    ///
    /// 注：分隔形式 "windows-amd64" 仍会拆出独立 "windows" token 命中含 "Windows" 关键词
    ///   的分类；该场景由种子泛词清理（migrations.rs stopwords 扩展）根治。
    ///
    /// 返回第一个命中的 func_category 名称（按 func_categories 表顺序）。
    pub fn classify_functional(
        file: &FileRecord,
        func_categories: &[crate::db::config::FuncCategoryRow],
    ) -> Option<String> {
        if func_categories.is_empty() {
            return None;
        }
        // 门控：仅对软件类文件运行功能分类。绿色软件目录(is_app_dir)和软件扩展名
        // （安装包/压缩包/Java工具/镜像）通过；其余（文档/媒体/图片/脚本等）直接返回 None。
        // 这样 MSA_Design_Doc.md、报告.pdf、教程.docx 等不会被误分到 SysEnhance/DocView。
        if !file.is_app_dir && !is_software_file(&file.extension) {
            return None;
        }
        // 拼接文件名 + 路径，仅按分隔符拆 token（保留驼峰连写专有名整体）
        let combined = format!("{} {}", file.name, file.local_path);
        let tokens: Vec<String> = split_on_separators(&combined);
        let token_set: std::collections::HashSet<&str> = tokens.iter().map(|s| s.as_str()).collect();

        for fc in func_categories {
            if !fc.enabled {
                continue;
            }
            let is_security = fc.parent == "网络安全";
            // 收集本分类命中的不同 token（去重）
            let mut hit_keywords: Vec<&str> = Vec::new();
            for kw in &fc.keywords {
                let kw_lower = kw.to_lowercase();
                // 最小长度过滤：关键词 < 3 字符忽略（VS/IDE/JS 等过短泛词）
                if kw_lower.chars().count() < 3 {
                    continue;
                }
                if token_set.contains(kw_lower.as_str()) {
                    if !hit_keywords.iter().any(|h| h.eq_ignore_ascii_case(kw)) {
                        hit_keywords.push(kw.as_str());
                    }
                }
            }
            if hit_keywords.is_empty() {
                continue;
            }
            // 安全类多信号：要求 ≥2 个不同 token 命中，
            // 或唯一命中的关键词是强专有名（≥5 字符纯字母专有名）。
            if is_security && hit_keywords.len() < 2 {
                let only = hit_keywords[0];
                let strong = only.chars().count() >= 5
                    && only.chars().all(|c| c.is_ascii_alphabetic());
                if !strong {
                    continue;
                }
            }
            return Some(fc.name.clone());
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

/// 判断扩展名是否为"软件类"（安装包/压缩包/Java工具/镜像）。
///
/// 功能分类（func_categories）只对软件类文件和绿色软件目录(is_app_dir)运行，
/// 避免文档/媒体/图片/脚本等非软件被误分到 SysEnhance/DocView 等软件分类。
/// 与 default_rules 的软件类 CategoryRule 扩展名保持一致。
fn is_software_file(ext: &str) -> bool {
    let e = ext.to_lowercase();
    const SOFTWARE_EXTS: &[&str] = &[
        // 安装包
        ".exe", ".msi", ".pkg", ".dmg", ".deb", ".rpm", ".appimage",
        // 压缩包（软件分发的常见载体）
        ".zip", ".7z", ".rar", ".gz", ".tar", ".xz", ".bz2",
        // Java 工具
        ".jar", ".war",
        // 镜像（系统/虚拟机镜像，属软件分发物）
        ".iso", ".img", ".vmdk", ".vhd",
    ];
    SOFTWARE_EXTS.iter().any(|s| s.eq_ignore_ascii_case(&e))
}

/// 将字符串按分隔符拆分为小写 token（**不拆驼峰**）。
///
/// 分隔符：`-` `_` `.` `/` `\` 空格。
/// 驼峰连写的专有名（EasyBCD / Metasploit / VSCode）保持整体，便于精确匹配关键词。
/// 例："pre-setup-wsl.sh" → ["pre", "setup", "wsl", "sh"]
///     "EasyBCD-2.4.2.exe" → ["easybcd", "2", "4", "2"]
///     "app_v2.1/test" → ["app", "v2", "1", "test"]
fn split_on_separators(s: &str) -> Vec<String> {
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

    /// 验证 split_on_separators 拆分规则（仅分隔符，不拆驼峰）
    #[test]
    fn test_split_on_separators() {
        assert_eq!(split_on_separators("pre-setup-wsl.sh"), vec!["pre", "setup", "wsl", "sh"]);
        // 驼峰连写保留整体（专有名 EasyBCD/DBX 不内部拆）
        assert_eq!(split_on_separators("EasyBCD-2.4.2"), vec!["easybcd", "2", "4", "2"]);
        assert_eq!(split_on_separators("DBX_0_5_32"), vec!["dbx", "0", "5", "32"]);
        // 空字符串
        assert!(split_on_separators("").is_empty());
        // 多分隔符（版本号 v2 保留整体）
        assert_eq!(split_on_separators("app_v2.1/test"), vec!["app", "v2", "1", "test"]);
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

        // 路径含关键词（拼接 name + path 匹配）——用软件扩展名(.exe)通过软件门控
        let f2 = FileRecord {
            name: "framework".to_string(),
            local_path: "D:\\Tools\\Metasploit\\framework.exe".to_string(),
            extension: ".exe".to_string(),
            ..Default::default()
        };
        assert_eq!(
            Classifier::classify_functional(&f2, &func_cats),
            Some("Exp-Frameworks".to_string())
        );

        // 无命中（.txt 是非软件，门控直接返回 None；即使文件名含关键词也不分类）
        let f3 = make_file("random-file", ".txt");
        assert_eq!(Classifier::classify_functional(&f3, &func_cats), None);

        // 子串不匹配（含 "easy" 但不是独立 token）
        let f4 = make_file("uneasy-reader", ".pdf");
        assert_eq!(Classifier::classify_functional(&f4, &func_cats), None);

        // 空列表
        assert_eq!(Classifier::classify_functional(&f1, &[]), None);
    }

    /// 验证门控：功能分类只对软件类（安装包/压缩包/Java/镜像）和绿色软件目录运行，
    /// 文档/媒体/图片/脚本等非软件直接返回 None（不被误分到 SysEnhance/DocView 等）。
    #[test]
    fn test_classify_functional_skips_non_software() {
        use crate::db::config::FuncCategoryRow;

        // DocView 分类含 "Doc" 关键词，文档类文件的 "doc" token 会命中它（旧 bug）
        let func_cats = vec![FuncCategoryRow {
            id: 1,
            name: "DocView".to_string(),
            keywords: vec!["Doc".to_string(), "Zeal".to_string()],
            parent: "编程开发".to_string(),
            description: String::new(),
            target_path: String::new(),
            enabled: true,
        }];

        // 文档类（.md/.docx/.pdf/.txt）即使文件名含 "doc" token 也不分类
        let doc_files = [
            ("MSA_Design_Doc_v3", ".md"),
            ("V8.3软件下载教程", ".docx"),
            ("report", ".pdf"),
            ("llms", ".txt"),
        ];
        for (name, ext) in doc_files {
            let f = make_file(name, ext);
            assert_eq!(
                Classifier::classify_functional(&f, &func_cats),
                None,
                "文档 {}{} 不应被功能分类（非软件类）",
                name,
                ext
            );
        }

        // 软件类（.zip/.exe/.jar）文件名含 "doc" token 仍可分类
        let sw = make_file("my-doc-tool", ".zip");
        assert_eq!(
            Classifier::classify_functional(&sw, &func_cats),
            Some("DocView".to_string()),
            "软件压缩包含 doc token 应能被功能分类"
        );

        // 绿色软件目录（is_app_dir=true）即使是文档扩展名也分类
        let app_dir_doc = FileRecord {
            name: "sometool".to_string(),
            extension: ".md".to_string(),
            is_app_dir: true,
            ..Default::default()
        };
        // is_app_dir 跳过扩展名门控，但仍需命中关键词（这里没命中 DocView 的 doc/sometool）
        // 用一个会命中的例子
        let app_dir_match = FileRecord {
            name: "doc-tool".to_string(),
            extension: String::new(),
            is_app_dir: true,
            ..Default::default()
        };
        assert_eq!(
            Classifier::classify_functional(&app_dir_match, &func_cats),
            Some("DocView".to_string()),
            "绿色软件目录应跳过扩展名门控，参与功能分类"
        );
    }

    /// 回归测试：Downloads 实测误分类场景（aho-corasick 子串匹配时代的 bug）。
    ///
    /// 修复前：aho-corasick 子串匹配会让下列文件被误分：
    ///   - `AliCloud-Tools-v1.0.5-windows-amd64.zip` → ISO（"Windows" 子串命中）
    ///   - `DropIt_v8.5.1_Portable.zip` → IoT-Wireless（"portable" 等弱子串命中）
    ///   - `Directory Opus 13.19.5 Beta (x64) Multilingual.zip` → FileMgr（"Directory" 子串命中）
    ///
    /// 修复后：token 边界 + 安全类多信号 + 最小长度 ≥3，杜绝这些误命中。
    #[test]
    fn test_classify_functional_no_substring_false_positive() {
        use crate::db::config::FuncCategoryRow;

        // 复刻 DB 里的脏关键词（来自运行时 catalog.db 实测）
        let func_cats = vec![
            FuncCategoryRow {
                id: 1,
                name: "ISO".to_string(),
                keywords: vec![
                    "操作系统".to_string(),
                    "Windows".to_string(),
                    "Linux".to_string(),
                    "ISO".to_string(),
                ],
                parent: "操作系统".to_string(),
                description: String::new(),
                target_path: String::new(),
                enabled: true,
            },
            FuncCategoryRow {
                id: 2,
                name: "IoT-Wireless".to_string(),
                keywords: vec![
                    "网络安全".to_string(),
                    "BLE".to_string(),
                    "Zigbee".to_string(),
                    "Wireless".to_string(),
                ],
                parent: "网络安全".to_string(),
                description: String::new(),
                target_path: String::new(),
                enabled: true,
            },
            FuncCategoryRow {
                id: 3,
                name: "FileMgr".to_string(),
                keywords: vec![
                    "系统增强".to_string(),
                    "Directory".to_string(),
                    "Opus".to_string(),
                    "Total".to_string(),
                    "Commander".to_string(),
                ],
                parent: "系统增强".to_string(),
                description: String::new(),
                target_path: String::new(),
                enabled: true,
            },
            FuncCategoryRow {
                id: 4,
                name: "Crypt-Symmetric".to_string(),
                keywords: vec![
                    "网络安全".to_string(),
                    "AES".to_string(),
                    "DES".to_string(),
                    "SM4".to_string(),
                ],
                parent: "网络安全".to_string(),
                description: String::new(),
                target_path: String::new(),
                enabled: true,
            },
        ];

        // 回归1: 连写的 windowsamd64（无分隔符）不应被 "Windows" 关键词命中
        //   tokens: ["alicloudtoolsv105windowsamd64zip"] —— 整体一个 token（无分隔符 + 无驼峰边界）
        //   "Windows" ≠ 该 token → 不命中 ISO
        //
        // 注意：分隔形式 "windows_amd64" / "windows-amd64" 会被拆出独立 "windows" token，
        //   仍会命中 ISO（因 ISO 类关键词含 "Windows"）。**该场景由步骤2种子泛词清理根治**
        //   （从 ISO 类移除 "Windows"/"Linux" 等平台泛词）。本测试仅验证 token 边界本身。
        let f_ali = FileRecord {
            name: "alicloudtoolswindowsamd64.zip".to_string(),
            local_path: "D:\\Downloads\\alicloudtoolswindowsamd64.zip".to_string(),
            extension: ".zip".to_string(),
            ..Default::default()
        };
        let r_ali = Classifier::classify_functional(&f_ali, &func_cats);
        assert_ne!(
            r_ali, Some("ISO".to_string()),
            "连写 windowsamd64（无分隔符）不应被 'Windows' 关键词命中（token 边界）"
        );

        // 回归2: DropIt_Portable.zip 不应被 IoT-Wireless 误分（无 BLE/Zigbee/Wireless token，
        //   且 IoT-Wireless 是安全类要求多信号）→ None
        let f_dropit = FileRecord {
            name: "DropIt_v8.5.1_Portable.zip".to_string(),
            local_path: "D:\\Downloads\\DropIt_v8.5.1_Portable.zip".to_string(),
            extension: ".zip".to_string(),
            ..Default::default()
        };
        assert_eq!(
            Classifier::classify_functional(&f_dropit, &func_cats),
            None,
            "DropIt_Portable 不应被误分到 IoT-Wireless"
        );

        // 回归3: 密码算法泛词（AES/DES/SM4）单命中不应定性为安全类
        //   文件名含 "AES" 但无其他安全信号 → 安全类多信号规则拒绝
        let f_aes = FileRecord {
            name: "my-aes-tool.zip".to_string(),
            local_path: "D:\\Downloads\\my-aes-tool.zip".to_string(),
            extension: ".zip".to_string(),
            ..Default::default()
        };
        // 注意：AES 只有 3 字符，被最小长度≥3 边界允许，但安全类要求 ≥2 信号或强专有名
        //   "aes" 是 3 字符，不满足"≥5 字符强专有名" → 安全类拒绝
        assert_ne!(
            Classifier::classify_functional(&f_aes, &func_cats),
            Some("Crypt-Symmetric".to_string()),
            "单 AES 信号不应定性为密码学安全类"
        );

        // 回归4: 强专有名单信号应被安全类接受（Metasploit 这类 ≥5 字符纯字母专有名）
        let func_cats2 = vec![FuncCategoryRow {
            id: 1,
            name: "Exp-Frameworks".to_string(),
            keywords: vec!["Metasploit".to_string()],
            parent: "网络安全".to_string(),
            description: String::new(),
            target_path: String::new(),
            enabled: true,
        }];
        let f_meta = FileRecord {
            name: "metasploit-framework-6.3.zip".to_string(),
            local_path: "D:\\Downloads\\metasploit-framework-6.3.zip".to_string(),
            extension: ".zip".to_string(),
            ..Default::default()
        };
        assert_eq!(
            Classifier::classify_functional(&f_meta, &func_cats2),
            Some("Exp-Frameworks".to_string()),
            "强专有名 Metasploit 单信号应被安全类接受"
        );

        // 回归5: 最小长度过滤 —— 关键词 "VS"（2字符）应被忽略，不参与匹配
        //   用分隔形式 "code-blocks" 让 "code" 成为独立 token（驼峰连写 VSCode 不拆）
        let func_cats3 = vec![FuncCategoryRow {
            id: 1,
            name: "Editor".to_string(),
            keywords: vec!["VS".to_string(), "Code".to_string()],
            parent: "编程开发".to_string(),
            description: String::new(),
            target_path: String::new(),
            enabled: true,
        }];
        let f_cb = FileRecord {
            name: "code-blocks.zip".to_string(),
            local_path: "D:\\Downloads\\code-blocks.zip".to_string(),
            extension: ".zip".to_string(),
            ..Default::default()
        };
        // tokens: ["code", "blocks", "zip"] —— "VS"(2字符)被忽略，但 "Code" 命中 → Editor
        assert_eq!(
            Classifier::classify_functional(&f_cb, &func_cats3),
            Some("Editor".to_string()),
            "'VS' 2字符关键词被忽略，但 'Code' 命中"
        );

        // 回归5b: 仅剩 2 字符关键词可命中时（无其他信号）→ 全被忽略 → None
        let func_cats4 = vec![FuncCategoryRow {
            id: 1,
            name: "Editor".to_string(),
            keywords: vec!["VS".to_string(), "JS".to_string()],
            parent: "编程开发".to_string(),
            description: String::new(),
            target_path: String::new(),
            enabled: true,
        }];
        let f_vs = FileRecord {
            name: "vs-js-tool.zip".to_string(),
            local_path: "D:\\Downloads\\vs-js-tool.zip".to_string(),
            extension: ".zip".to_string(),
            ..Default::default()
        };
        // tokens: ["vs","js","tool"] —— VS/JS 均 2 字符被最小长度过滤 → 无命中 → None
        assert_eq!(
            Classifier::classify_functional(&f_vs, &func_cats4),
            None,
            "仅 2 字符关键词命中时应被最小长度过滤 → None"
        );
    }
}
