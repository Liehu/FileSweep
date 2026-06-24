//! 智能建议引擎
//!
//! 基于决策矩阵为每个文件生成建议（保留/降级为链接/删除旧版/删除重复）。
//! 数据源：FileRecord + CatalogEntry（AI 丰富）+ 内置知名度表。

use crate::core::models::{CatalogEntry, DedupGroup, FileRecord};

// ────────────────── 内置知名度表 ──────────────────

/// 常见开发者软件映射（软件名关键词 → 可安全降级为链接）
/// 这些软件的官网稳定、下载可靠，删除后随时能重新下载。
const KNOWN_SOFTWARE: &[(&str, &str)] = &[
    // 开发工具
    ("python", "python.org"),
    ("node", "nodejs.org"),
    ("npm", "nodejs.org"),
    ("git", "git-scm.com"),
    ("java", "oracle.com"),
    ("jdk", "oracle.com"),
    ("go.", "go.dev"),
    ("rust", "rust-lang.org"),
    ("golang", "go.dev"),
    ("docker", "docker.com"),
    ("maven", "maven.apache.org"),
    ("gradle", "gradle.org"),
    ("visual studio code", "code.visualstudio.com"),
    ("vscode", "code.visualstudio.com"),
    ("intellij", "jetbrains.com"),
    ("pycharm", "jetbrains.com"),
    ("webstorm", "jetbrains.com"),
    ("goland", "jetbrains.com"),
    ("android studio", "developer.android.com"),
    ("eclipse", "eclipse.org"),
    // 系统工具
    ("7-zip", "7-zip.org"),
    ("7zip", "7-zip.org"),
    ("everything", "voidtools.com"),
    ("powertoys", "microsoft.com"),
    ("putty", "putty.org"),
    ("windirstat", "windirstat.net"),
    // 浏览器
    ("chrome", "google.com/chrome"),
    ("firefox", "mozilla.org"),
    // 通信/办公
    ("obsidian", "obsidian.md"),
    ("notion", "notion.so"),
    // 数据库工具
    ("dbeaver", "dbeaver.io"),
    ("navicat", "navicat.com"),
    // 其他
    ("vlc", "videolan.org"),
    ("ffmpeg", "ffmpeg.org"),
    ("wireshark", "wireshark.org"),
];

/// 判断文件是否匹配内置知名软件
fn match_known_software(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    for (keyword, url) in KNOWN_SOFTWARE {
        if lower.contains(keyword) {
            return Some(url);
        }
    }
    None
}

// ────────────────── 建议结构 ──────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SuggestionItem {
    pub file_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_size: i64,
    pub category: String, // 文件分类
    pub suggestion: String, // "keep" / "downgrade" / "delete_old" / "delete_dup"
    pub confidence: String, // "high" / "medium" / "low"
    pub reason: String,
    pub homepage_url: String,   // 官网入口（降级为链接时用）
    pub auto_checked: bool,     // 是否默认勾选
    pub keep_id: Option<String>,
    pub keep_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SuggestionSummary {
    pub total_items: usize,
    pub total_size: i64,
    pub high_confidence: Vec<SuggestionItem>,
    pub medium_confidence: Vec<SuggestionItem>,
    pub old_versions: Vec<SuggestionItem>,
    pub duplicates: Vec<SuggestionItem>,
    pub kept: usize, // 不显示的"保留"项数量
}

// ────────────────── 建议引擎 ──────────────────

/// 生成智能建议
///
/// 输入：文件记录 + catalog 条目（AI 丰富）+ 去重组（版本/重复）
/// 输出：分组建议（高置信/需确认/旧版本/重复）
pub fn generate_suggestions(
    records: &[FileRecord],
    catalogs: &[CatalogEntry],
    dup_groups: &[DedupGroup],
) -> SuggestionSummary {
    // 构建 catalog 索引（按 catalog_id 或 name 匹配）
    let catalog_by_name: std::collections::HashMap<String, &CatalogEntry> = catalogs
        .iter()
        .map(|c| (c.name.to_lowercase(), c))
        .collect();

    // 构建去重组的"应删除"文件 ID 集合
    let mut old_version_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut duplicate_ids: std::collections::HashSet<String> = std::collections::HashSet::new();

    for group in dup_groups {
        for dup in &group.duplicates {
            if group.reason == "multi_version" {
                old_version_ids.insert(dup.id.clone());
            } else {
                duplicate_ids.insert(dup.id.clone());
            }
        }
    }

    let mut high = Vec::new();
    let mut medium = Vec::new();
    let mut old_versions = Vec::new();
    let mut duplicates = Vec::new();
    let mut kept = 0usize;

    for r in records {
        // 1. 绿色软件目录 → 保留
        if r.is_app_dir {
            kept += 1;
            continue;
        }

        // 2. 旧版本 → 删除旧版（高置信）
        if old_version_ids.contains(&r.id) {
            let rep = dup_groups
                .iter()
                .find(|g| g.duplicates.iter().any(|d| d.id == r.id))
                .map(|g| &g.representative);
            old_versions.push(SuggestionItem {
                file_id: r.id.clone(),
                file_name: r.name.clone(),
                file_path: r.local_path.clone(),
                file_size: r.file_size,
                category: r.category.clone(),
                suggestion: "delete_old".into(),
                confidence: "high".into(),
                reason: format!(
                    "旧版本，建议删除（保留 {}）",
                    rep.map(|p| p.name.as_str()).unwrap_or("最新版")
                ),
                homepage_url: String::new(),
                auto_checked: true,
                keep_id: rep.map(|p| p.id.clone()),
                keep_name: rep.map(|p| p.name.clone()),
            });
            continue;
        }

        // 3. 重复文件 → 删除副本（高置信）
        if duplicate_ids.contains(&r.id) {
            let rep = dup_groups
                .iter()
                .find(|g| g.duplicates.iter().any(|d| d.id == r.id))
                .map(|g| &g.representative);
            duplicates.push(SuggestionItem {
                file_id: r.id.clone(),
                file_name: r.name.clone(),
                file_path: r.local_path.clone(),
                file_size: r.file_size,
                category: r.category.clone(),
                suggestion: "delete_dup".into(),
                confidence: "high".into(),
                reason: "完全重复文件，建议删除副本".into(),
                homepage_url: String::new(),
                auto_checked: true,
                keep_id: rep.map(|p| p.id.clone()),
                keep_name: rep.map(|p| p.name.clone()),
            });
            continue;
        }

        // 4. 查找 catalog 条目（AI 丰富数据）
        let catalog = catalog_by_name.get(&r.name.to_lowercase());

        // 5. 安装包/可下载文件 → 降级为链接
        let is_installer = is_installer_file(&r.extension);
        if is_installer || is_downloadable_archive(&r.extension) {
            let known = match_known_software(&r.name);

            // 有官网入口（内置表或 AI 补全）
            let homepage = known.map(|s| s.to_string()).or_else(|| {
                catalog.and_then(|c| {
                    if !c.homepage_url.is_empty() {
                        Some(c.homepage_url.clone())
                    } else {
                        None
                    }
                })
            });

            if let Some(url) = homepage {
                let is_known = known.is_some();
                let ai_confidence = catalog.map(|c| c.ai_confidence).unwrap_or(0.0);

                let item = SuggestionItem {
                    file_id: r.id.clone(),
                    file_name: r.name.clone(),
                    file_path: r.local_path.clone(),
                    file_size: r.file_size,
                    category: r.category.clone(),
                    suggestion: "downgrade".into(),
                    confidence: if is_known { "high" } else { "medium" }.into(),
                    reason: if is_known {
                        format!("知名软件安装包，可从 {} 重新下载", url)
                    } else {
                        format!("AI 识别可从 {} 下载，建议确认", url)
                    },
                    homepage_url: url,
                    auto_checked: is_known, // 知名软件自动勾选，非知名需确认
                    keep_id: None,
                    keep_name: None,
                };

                // AI confidence 修正：低置信度的也标 medium
                if !is_known && ai_confidence > 0.3 && ai_confidence < 0.7 {
                    // 保持 medium
                }

                if is_known {
                    high.push(item);
                } else {
                    medium.push(item);
                }
                continue;
            }
        }

        // 6. 个人文档/照片/视频 → 保留
        if is_personal_file(&r.extension) {
            kept += 1;
            continue;
        }

        // 7. 其他文件 → 保留（保守策略）
        kept += 1;
    }

    let total_items = high.len() + medium.len() + old_versions.len() + duplicates.len();
    let total_size: i64 = high.iter()
        .chain(medium.iter())
        .chain(old_versions.iter())
        .chain(duplicates.iter())
        .map(|i| i.file_size)
        .sum();

    SuggestionSummary {
        total_items,
        total_size,
        high_confidence: high,
        medium_confidence: medium,
        old_versions,
        duplicates,
        kept,
    }
}

// ────────────────── 辅助判定 ──────────────────

fn is_installer_file(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        ".msi" | ".msix" | ".msixbundle" | ".appx" | ".appxbundle"
    )
}

fn is_downloadable_archive(ext: &str) -> bool {
    let l = ext.to_lowercase();
    l == ".exe"
        || l == ".zip"
        || l == ".7z"
        || l == ".rar"
        || l == ".gz"
        || l == ".tar"
        || l == ".dmg"
        || l == ".deb"
        || l == ".rpm"
}

fn is_personal_file(ext: &str) -> bool {
    let l = ext.to_lowercase();
    matches!(
        l.as_str(),
        ".doc" | ".docx" | ".pdf" | ".ppt" | ".pptx" | ".xls" | ".xlsx"
        | ".odt" | ".rtf" | ".epub"
        | ".jpg" | ".jpeg" | ".png" | ".gif" | ".bmp" | ".tiff" | ".webp" | ".svg" | ".heic"
        | ".mp4" | ".mkv" | ".avi" | ".mov" | ".wmv" | ".m4v"
        | ".mp3" | ".flac" | ".wav" | ".aac" | ".m4a"
        | ".psd" | ".ai" | ".xd" | ".fig"
    )
}
