//! 智能建议引擎
//!
//! 基于决策矩阵为每个文件生成建议（保留/降级为链接/删除旧版/删除重复）。
//! 数据源：FileRecord + CatalogEntry（AI 丰富）+ 内置知名度表。

use crate::core::models::{CatalogEntry, DedupGroup, FileRecord};
use crate::db::config::FuncCategoryRow;

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
    pub suggestion: String, // "keep" / "downgrade" / "delete_old" / "delete_dup" / "delete" / "move"
    pub confidence: String, // "high" / "medium" / "low"
    pub reason: String,
    pub homepage_url: String,   // 官网入口（降级为链接时用）
    pub auto_checked: bool,     // 是否默认勾选
    pub keep_id: Option<String>,
    pub keep_name: Option<String>,
    /// 迁移目标路径（suggestion=move 时），相对路径由 executor 拼 migrate_root_dir
    #[serde(default)]
    pub move_target: String,
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
/// 输入：文件记录 + catalog 条目（AI 丰富）+ 去重组（版本/重复）+ 功能分类表
/// 输出：分组建议（高置信/需确认/旧版本/重复）
///
/// `func_categories`：用于"按功能用途整理到细分目录"——当文件已通过关键词分类器
/// 或 AI 补全获得 functional_category，且该分类有 target_path（如 Security\Exploit\Frameworks），
/// 生成 move 建议把文件整理到对应细分目录。executor 的 resolve_dest 会把相对路径拼到 migrate_root。
pub fn generate_suggestions(
    records: &[FileRecord],
    catalogs: &[CatalogEntry],
    dup_groups: &[DedupGroup],
    func_categories: &[FuncCategoryRow],
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
        // 0. 目录迁移：聚合目录（is_app_dir）带 move_target → 建议迁移（需用户确认，不自动勾选）
        // 整目录搬到规则指定的目标路径，executor 的 move_dir 负责实际移动
        if r.is_app_dir && !r.move_target.is_empty() {
            medium.push(SuggestionItem {
                file_id: r.id.clone(),
                file_name: r.name.clone(),
                file_path: r.local_path.clone(),
                file_size: r.file_size,
                category: r.category.clone(),
                suggestion: "move".into(),
                confidence: "medium".into(),
                reason: format!("目录类型 {}，建议迁移到 {}", r.app_dir_reason, r.move_target),
                homepage_url: String::new(),
                auto_checked: false, // 迁移不可逆性较高，需用户确认
                keep_id: None,
                keep_name: None,
                move_target: r.move_target.clone(),
            });
            continue;
        }

        // 0.5 按功能用途整理到细分目录：文件已有 functional_category 且该分类有 target_path
        //     （关键词分类器或 AI 补全打的分类），建议 move 到 fc.target_path（如 Security\Exploit\Frameworks）。
        //     这是用户"按功能用途分类到细分目录"诉求的落地：executor.resolve_dest 把相对路径拼到 migrate_root。
        //     置信度规则：AI confidence≥0.8 或 download_reliability==high → medium；否则 → low（需确认）。
        //     不自动勾选（移动不可逆，与目录迁移一致）。
        if !r.is_app_dir && !r.functional_category.is_empty() {
            if let Some(fc) = func_categories.iter().find(|c| c.name == r.functional_category) {
                if !fc.target_path.is_empty() {
                    // 查 catalog 条目确定置信度（AI confidence / download_reliability）
                    let cat_entry = catalog_by_name.get(&r.name.to_lowercase());
                    let ai_conf = cat_entry.map(|c| c.ai_confidence).unwrap_or(0.0);
                    let reliable_high = cat_entry
                        .map(|c| c.download_reliability == "high")
                        .unwrap_or(false);
                    let is_medium = ai_conf >= 0.8 || reliable_high;
                    // 与现有规则一致：无独立 low 分组，low 标记的项也放入 medium 分组（带 low 标签提示）
                    let confidence = if is_medium { "medium" } else { "low" };
                    let reason = format!(
                        "功能分类 {}（{}），建议整理到 {}",
                        fc.name, fc.parent, fc.target_path
                    );
                    medium.push(SuggestionItem {
                        file_id: r.id.clone(),
                        file_name: r.name.clone(),
                        file_path: r.local_path.clone(),
                        file_size: r.file_size,
                        category: r.category.clone(),
                        suggestion: "move".into(),
                        confidence: confidence.into(),
                        reason,
                        homepage_url: String::new(),
                        auto_checked: false, // 移动不可逆，需用户确认
                        keep_id: None,
                        keep_name: None,
                        move_target: fc.target_path.clone(),
                    });
                    continue;
                }
            }
        }

        // 1. 绿色软件目录 → 保留
        if r.is_app_dir {
            kept += 1;
            continue;
        }

        // 1.5 临时文件 → 建议删除（高置信，扫描器对 TEMP_FILES 子树内文件打了标记）
        if r.app_dir_reason == "TEMP_FILES" {
            high.push(SuggestionItem {
                file_id: r.id.clone(),
                file_name: r.name.clone(),
                file_path: r.local_path.clone(),
                file_size: r.file_size,
                category: r.category.clone(),
                suggestion: "delete".into(),
                confidence: "high".into(),
                reason: "临时文件（无意义文件名），建议删除".into(),
                homepage_url: String::new(),
                auto_checked: true,
                keep_id: None,
                keep_name: None,
                move_target: String::new(),
            });
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
                move_target: String::new(),
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
                move_target: String::new(),
            });
            continue;
        }

        // 4. 查找 catalog 条目（AI 丰富数据）
        let catalog = catalog_by_name.get(&r.name.to_lowercase());

        // 5. 安装包/可下载文件 → 降级为链接
        let is_installer = is_installer_file(&r.extension);
        if is_installer || is_downloadable_archive(&r.extension) {
            let known = match_known_software(&r.name);

            // AI 补全的下载可靠性（catalog.download_reliability）
            let reliability = catalog
                .map(|c| c.download_reliability.as_str())
                .unwrap_or("");

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

                // 综合判定置信度：
                // - 内置知名度表 → high
                // - AI reliability = high → high（官方源，可安全删除重下）
                // - AI reliability = medium → medium
                // - AI reliability = low → low（来源不明，建议先备份）
                // - 其余（无可靠性评估） → medium
                let (confidence, auto_checked, reason) = if is_known {
                    ("high", true, format!("知名软件安装包，可从 {} 重新下载", url))
                } else {
                    match reliability {
                        "high" => (
                            "high",
                            true,
                            format!("AI 判定官方/可靠源，可从 {} 重新下载", url),
                        ),
                        "low" => (
                            "low",
                            false,
                            format!("AI 判定下载来源不明（{}），删除前建议先备份", url),
                        ),
                        _ => (
                            "medium",
                            false,
                            format!("AI 识别可从 {} 下载，建议确认", url),
                        ),
                    }
                };

                let item = SuggestionItem {
                    file_id: r.id.clone(),
                    file_name: r.name.clone(),
                    file_path: r.local_path.clone(),
                    file_size: r.file_size,
                    category: r.category.clone(),
                    suggestion: "downgrade".into(),
                    confidence: confidence.into(),
                    reason,
                    homepage_url: url,
                    auto_checked,
                    keep_id: None,
                    keep_name: None,
                    move_target: String::new(),
                };

                match confidence {
                    "high" => high.push(item),
                    "low" => {
                        // 低可靠性也放入 medium 分组，但保留 low 标记以提示用户
                        medium.push(item);
                    }
                    _ => medium.push(item),
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
