//! 目录类型识别（两层方案层 1）
//!
//! 在 find_app_roots 之前执行：先识别已知类型的目录 → 整目录聚合保留。
//! 评分模型只处理 APP_DIR/UNKNOWN 目录。
//!
//! 设计见 docs/superpowers/specs/2026-06-25-dir-classification-design.md

use crate::core::appdir::SubtreeStats;
use crate::db::config::DirPatternRow;
use std::collections::HashSet;
use std::path::Path;

// ────────────────── 目录类型枚举 ──────────────────

/// 目录类型。`as_str()` 返回的字符串会写入 file_records.app_dir_reason，
/// 并作为前端筛选与服务端 dir_type 过滤的 key。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirType {
    CodeProject,
    NoteCollection,
    YamlLibrary,
    CtfChallenge,
    KnowledgeBase,
    SampleCollection,
    TrainingMaterial,
    VulnMaterial,
    DocCollection,
    TempFiles,
    AppDir,   // 交给评分模型
    Unknown,  // 交给评分模型
}

impl DirType {
    /// 与 DB dir_type 字符串、app_dir_reason 字段互转
    pub fn as_str(&self) -> &'static str {
        match self {
            DirType::CodeProject => "CODE_PROJECT",
            DirType::NoteCollection => "NOTE_COLLECTION",
            DirType::YamlLibrary => "YAML_LIBRARY",
            DirType::CtfChallenge => "CTF_CHALLENGE",
            DirType::KnowledgeBase => "KNOWLEDGE_BASE",
            DirType::SampleCollection => "SAMPLE_COLLECTION",
            DirType::TrainingMaterial => "TRAINING_MATERIAL",
            DirType::VulnMaterial => "VULN_MATERIAL",
            DirType::DocCollection => "DOC_COLLECTION",
            DirType::TempFiles => "TEMP_FILES",
            DirType::AppDir => "APP_DIR",
            DirType::Unknown => "UNKNOWN",
        }
    }

    /// 从 DB dir_type 字符串解析；未知值回退 Unknown
    pub fn from_str(s: &str) -> DirType {
        match s {
            "CODE_PROJECT" => DirType::CodeProject,
            "NOTE_COLLECTION" => DirType::NoteCollection,
            "YAML_LIBRARY" => DirType::YamlLibrary,
            "CTF_CHALLENGE" => DirType::CtfChallenge,
            "KNOWLEDGE_BASE" => DirType::KnowledgeBase,
            "SAMPLE_COLLECTION" => DirType::SampleCollection,
            "TRAINING_MATERIAL" => DirType::TrainingMaterial,
            "VULN_MATERIAL" => DirType::VulnMaterial,
            "DOC_COLLECTION" => DirType::DocCollection,
            "TEMP_FILES" => DirType::TempFiles,
            "APP_DIR" => DirType::AppDir,
            "UNKNOWN" => DirType::Unknown,
            _ => DirType::Unknown,
        }
    }

    /// 是否为已知类型（应聚合保留，不交给评分模型）
    pub fn is_known_keep_type(&self) -> bool {
        matches!(
            self,
            DirType::CodeProject
                | DirType::NoteCollection
                | DirType::YamlLibrary
                | DirType::CtfChallenge
                | DirType::KnowledgeBase
                | DirType::SampleCollection
                | DirType::TrainingMaterial
                | DirType::VulnMaterial
                | DirType::DocCollection
        )
    }

    /// 前端展示用的中文标签
    pub fn label(&self) -> &'static str {
        match self {
            DirType::CodeProject => "代码项目",
            DirType::NoteCollection => "笔记",
            DirType::YamlLibrary => "YAML库",
            DirType::CtfChallenge => "CTF题目",
            DirType::KnowledgeBase => "知识库",
            DirType::SampleCollection => "样本集合",
            DirType::TrainingMaterial => "培训资料",
            DirType::VulnMaterial => "漏洞资料",
            DirType::DocCollection => "文档集合",
            DirType::TempFiles => "临时文件",
            DirType::AppDir => "应用目录",
            DirType::Unknown => "未识别",
        }
    }

    /// 由 reason 字符串（app_dir_reason）反查中文标签。
    /// 兼容旧值（exe-app/jar-app/python-project/software_root 等）。
    pub fn label_from_reason(reason: &str) -> String {
        if reason.is_empty() {
            return String::new();
        }
        // 先尝试按 DirType 解析
        let dt = DirType::from_str(reason);
        if dt != DirType::Unknown || reason == "UNKNOWN" {
            return dt.label().to_string();
        }
        // 旧值兼容
        match reason {
            "exe-app" => "应用目录".to_string(),
            "jar-app" => "Java应用".to_string(),
            "python-project" => "Python项目".to_string(),
            "software_root" => "软件目录".to_string(),
            other => other.to_string(),
        }
    }
}

// ────────────────── 输入切片 ──────────────────

/// 分类器需要的目录信息（scanner 把私有 DirNode 投影为这个切片，避免暴露内部类型）。
pub struct DirInput<'a> {
    /// 目录名（file_name，小写）
    pub name: String,
    /// 直接子文件名
    pub files: &'a [String],
    /// 直接子目录名（小写，用于同名文件夹检测）
    pub child_dir_names: &'a [String],
    /// 子树统计（含子目录聚合）
    pub stats: &'a SubtreeStats,
}

// ────────────────── 主分类入口 ──────────────────

/// 完整的分类结果：类型 + 命中规则的处理动作与目标路径。
///
/// 扫描器据此决定：keep 类型聚合保留；move 类型聚合并写入 move_target；
/// delete（TEMP_FILES）标记子树；app_dir/unknown 交给评分模型。
#[derive(Debug, Clone)]
pub struct DirClassification {
    pub dir_type: DirType,
    /// 命中规则的动作：keep / delete / move / app_dir。
    /// 指纹兜底命中的类型，action 默认为 keep（TEMP_FILES 为 delete）。
    pub action: String,
    /// action=move 时的迁移目标路径（相对或绝对），其余动作为空。
    pub target_path: String,
}

impl DirClassification {
    fn from_type(dt: DirType) -> Self {
        let action = if dt == DirType::TempFiles { "delete" } else { "keep" };
        Self {
            dir_type: dt,
            action: action.into(),
            target_path: String::new(),
        }
    }

    fn from_pattern(dt: DirType, p: &DirPatternRow) -> Self {
        Self {
            dir_type: dt,
            action: p.action.clone(),
            target_path: p.target_path.clone(),
        }
    }
}

/// 对目录分类（仅返回类型，向后兼容）。
pub fn classify_dir_type(dir: &Path, input: &DirInput, patterns: &[DirPatternRow]) -> DirType {
    classify_dir_full(dir, input, patterns).dir_type
}

/// 完整分类：返回类型 + 动作 + 目标路径（扫描器消费此结果决定聚合/迁移/删除）。
///
/// 优先级 1: file_markers（dir_patterns 表）
/// 优先级 2: dir_name_keywords（dir_patterns 表）
/// 优先级 3: 文件类型指纹（内置兜底：NOTE/YAML/TEMP/DOC）
/// 优先级 4/5: APP_DIR / UNKNOWN（交给评分模型）
pub fn classify_dir_full(dir: &Path, input: &DirInput, patterns: &[DirPatternRow]) -> DirClassification {
    let dir_name_lower = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let files_lower: Vec<String> = input.files.iter().map(|f| f.to_lowercase()).collect();

    // 优先级 1: file_markers（dir_patterns 表，按 priority 升序已排序）
    for p in patterns
        .iter()
        .filter(|p| p.enabled && !p.file_markers.is_empty())
    {
        if p.file_markers
            .iter()
            .any(|m| files_lower.contains(&m.to_lowercase()))
        {
            // require_no_exec=true 且目录含可执行文件 → 该 pattern 不命中，继续匹配下一条
            // （防止"笔记目录里被塞了 exe"被误判为安全类型而吞掉 exe）
            if p.require_no_exec && input.stats.has_exec() {
                continue;
            }
            return DirClassification::from_pattern(DirType::from_str(&p.dir_type), p);
        }
    }

    // 优先级 2: dir_name_keywords（dir_patterns 表）
    for p in patterns
        .iter()
        .filter(|p| p.enabled && !p.dir_name_keywords.is_empty())
    {
        if p.dir_name_keywords
            .iter()
            .any(|kw| dir_name_lower.contains(&kw.to_lowercase()))
        {
            if p.require_no_exec && input.stats.has_exec() {
                continue;
            }
            return DirClassification::from_pattern(DirType::from_str(&p.dir_type), p);
        }
    }

    // 优先级 3: 文件类型指纹（内置兜底）
    if let Some(dt) = classify_by_file_fingerprint(input) {
        return DirClassification::from_type(dt);
    }

    // 优先级 4: APP_DIR / 优先级 5: UNKNOWN（交给评分模型）
    if input.stats.is_app_candidate() {
        DirClassification::from_type(DirType::AppDir)
    } else {
        DirClassification::from_type(DirType::Unknown)
    }
}

// ────────────────── 文件类型指纹（内置兜底）──────────────────

/// 内置指纹检测。返回 Some(DirType) 表示命中，None 表示未命中（交给评分模型）。
///
/// 规则（设计文档第 2 节优先级 3）：
/// - NOTE_COLLECTION: ≥1 个 .md + (同名文件夹 或 assets 目录 或 md+图片合计 > 50%) + 无 exe
/// - YAML_LIBRARY: .yaml/.yml 占比 > 60% + 无 exe
/// - TEMP_FILES: 无意义文件名占比 > 50%
/// - DOC_COLLECTION: .docx/.pptx/.pdf/.xlsx 占比 > 60% + 无 exe
fn classify_by_file_fingerprint(input: &DirInput) -> Option<DirType> {
    let total = input.stats.total_files;
    let has_exec = input.stats.has_exec();
    let files = input.files;

    // TEMP_FILES：无意义名占比 > 50%（独立于 exec，临时文件可能含任何后缀）
    if total > 0 {
        let meaningless = files.iter().filter(|f| is_meaningless_name(f)).count();
        // 占比基于直接子文件（子树聚合值 stats.total_files 含子目录，这里用直接文件更准确）
        if !files.is_empty() {
            let ratio = meaningless as f64 / files.len() as f64;
            if ratio > 0.5 {
                return Some(DirType::TempFiles);
            }
        }
    }

    // 以下类型均要求无 exe
    if has_exec {
        return None;
    }
    if total == 0 && files.is_empty() {
        return None;
    }

    // 直接子文件扩展名统计
    let md_count = count_by_ext(files, |e| e == ".md");
    let yaml_count = count_by_ext(files, |e| e == ".yaml" || e == ".yml");
    let doc_count = count_by_ext(files, |e| {
        matches!(e, ".docx" | ".pptx" | ".pdf" | ".xlsx" | ".doc" | ".ppt" | ".xls")
    });
    let image_count = count_by_ext(files, |e| {
        matches!(e, ".jpg" | ".jpeg" | ".png" | ".gif" | ".bmp" | ".webp" | ".svg")
    });
    let direct_total = files.len().max(1);

    // NOTE_COLLECTION: ≥1 md + (同名文件夹 / assets / md+图 > 50%)
    if md_count >= 1 {
        let has_same_name_dir = has_same_name_attachment_dir(files, input.child_dir_names);
        let has_assets = input
            .child_dir_names
            .iter()
            .any(|d| d == "assets" || d == "images" || d == "img" || d == "attachments");
        let md_image_ratio = (md_count + image_count) as f64 / direct_total as f64;
        if has_same_name_dir || has_assets || md_image_ratio > 0.5 {
            return Some(DirType::NoteCollection);
        }
    }

    // YAML_LIBRARY: yaml 占比 > 60%
    if direct_total > 0 && yaml_count as f64 / direct_total as f64 > 0.6 {
        return Some(DirType::YamlLibrary);
    }

    // DOC_COLLECTION: doc 类占比 > 60%
    if direct_total > 0 && doc_count as f64 / direct_total as f64 > 0.6 {
        return Some(DirType::DocCollection);
    }

    None
}

/// 统计满足扩展名谓词的文件数（直接子文件，小写比较）
fn count_by_ext<F: Fn(&str) -> bool>(files: &[String], pred: F) -> usize {
    files
        .iter()
        .filter(|f| {
            let lower = f.to_lowercase();
            let ext = match lower.rfind('.') {
                Some(i) => &lower[i..],
                None => "",
            };
            pred(ext)
        })
        .count()
}

/// 是否存在与某 .md 同名的附件文件夹（如 note.md → note/）
fn has_same_name_attachment_dir(files: &[String], child_dirs: &[String]) -> bool {
    let dir_set: HashSet<&str> = child_dirs.iter().map(|s| s.as_str()).collect();
    for f in files {
        let lower = f.to_lowercase();
        if lower.ends_with(".md") {
            let stem = &lower[..lower.len() - 3];
            if !stem.is_empty() && dir_set.contains(stem) {
                return true;
            }
        }
    }
    false
}

// ────────────────── 临时文件名判定 ──────────────────

/// 判断文件名是否"无意义"（设计文档第 4 节）。
///
/// - 纯数字且 ≤4 位（1.txt / 123.py）
/// - stem ≤ 2 字符 **且** 为代码类/临时后缀（a.h / bb.c / 1.py）——
///   避免把笔记类的 a.md / b.md 误判为临时文件
/// - temp / tmp 前缀（temp*.txt / tmp_cache.bin）
/// - 32 位十六进制（哈希/UUID 无连字符）
pub fn is_meaningless_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("");
    if stem.is_empty() {
        return false;
    }
    // 纯数字且 ≤4 位（任意后缀，1.txt / 123.bin 都是典型临时命名）
    if stem.len() <= 4 && stem.chars().all(|c| c.is_ascii_digit()) {
        return true;
    }
    // stem ≤ 2 字符 + 代码/临时类后缀（a.h / bb.c / x.py）
    if stem.chars().count() <= 2 && is_draft_extension(name) {
        return true;
    }
    // temp / tmp 前缀（不区分大小写）
    let stem_lower = stem.to_lowercase();
    if stem_lower.starts_with("temp") || stem_lower.starts_with("tmp") {
        return true;
    }
    // 32 位十六进制（哈希/UUID 无连字符）
    if stem.len() == 32 && stem.chars().all(|c| c.is_ascii_hexdigit()) {
        return true;
    }
    false
}

/// 草稿/代码类后缀：短 stem 搭配这些后缀视为临时（a.h / bb.c / 1.py / x.log）
fn is_draft_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    const DRAFT_EXTS: &[&str] = &[
        ".c", ".h", ".cc", ".cpp", ".cxx", ".hpp",
        ".py", ".js", ".ts", ".go", ".rs", ".java", ".rb", ".pl", ".lua",
        ".sh", ".bat", ".cmd", ".ps1",
        ".log", ".tmp", ".bak", ".out",
    ];
    DRAFT_EXTS.iter().any(|e| lower.ends_with(e))
}

// ────────────────── 测试 ──────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(exec: usize, total: usize) -> SubtreeStats {
        SubtreeStats {
            exec_count: exec,
            total_files: total,
            ..Default::default()
        }
    }

    fn input<'a>(name: &str, files: &'a [String], dirs: &'a [String], st: &'a SubtreeStats) -> DirInput<'a> {
        DirInput {
            name: name.to_string(),
            files,
            child_dir_names: dirs,
            stats: st,
        }
    }

    #[test]
    fn test_dir_type_roundtrip() {
        for dt in [
            DirType::CodeProject,
            DirType::NoteCollection,
            DirType::YamlLibrary,
            DirType::CtfChallenge,
            DirType::KnowledgeBase,
            DirType::SampleCollection,
            DirType::TrainingMaterial,
            DirType::VulnMaterial,
            DirType::DocCollection,
            DirType::TempFiles,
            DirType::AppDir,
            DirType::Unknown,
        ] {
            assert_eq!(DirType::from_str(dt.as_str()), dt);
        }
        assert_eq!(DirType::from_str("BOGUS"), DirType::Unknown);
    }

    #[test]
    fn test_label_from_reason_compat() {
        assert_eq!(DirType::label_from_reason("CODE_PROJECT"), "代码项目");
        assert_eq!(DirType::label_from_reason("exe-app"), "应用目录");
        assert_eq!(DirType::label_from_reason("python-project"), "Python项目");
        assert_eq!(DirType::label_from_reason("software_root"), "软件目录");
        assert_eq!(DirType::label_from_reason(""), "");
        assert_eq!(DirType::label_from_reason("custom_x"), "custom_x");
    }

    // ── is_meaningless_name ──

    #[test]
    fn test_meaningless_numeric() {
        assert!(is_meaningless_name("1.txt"));
        assert!(is_meaningless_name("123.py"));
        assert!(is_meaningless_name("9999"));
        assert!(!is_meaningless_name("12345.txt")); // 5 位数字不算
    }

    #[test]
    fn test_meaningless_short_stem() {
        // 短 stem + 代码/草稿后缀 → 无意义
        assert!(is_meaningless_name("a.h"));
        assert!(is_meaningless_name("bb.c"));
        assert!(is_meaningless_name("1.py"));
        assert!(is_meaningless_name("x.log"));
        // 短 stem 但无后缀或笔记类后缀 → 不算无意义（避免误判 a.md 笔记）
        assert!(!is_meaningless_name("x"));
        assert!(!is_meaningless_name("a.md"));
        assert!(!is_meaningless_name("readme.md"));
    }

    #[test]
    fn test_meaningless_temp_prefix() {
        assert!(is_meaningless_name("temp.txt"));
        assert!(is_meaningless_name("TEMP_old.log"));
        assert!(is_meaningless_name("tmp_cache.bin")); // tmp 不算（要求 temp 前缀）
    }

    #[test]
    fn test_meaningless_hex_hash() {
        assert!(is_meaningless_name("a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4.txt"));
        assert!(!is_meaningless_name("a1b2c3d4")); // 太短
        assert!(!is_meaningless_name("g1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4")); // 含非 hex
    }

    // ── classify_by_file_fingerprint ──

    #[test]
    fn test_fingerprint_temp_files() {
        let files: Vec<String> = vec!["1.txt".into(), "2.txt".into(), "3.py".into(), "readme.md".into()];
        let st = stats(0, 4);
        let inp = input("tmpdir", &files, &[], &st);
        assert_eq!(classify_by_file_fingerprint(&inp), Some(DirType::TempFiles));
    }

    #[test]
    fn test_fingerprint_note_collection_same_name_dir() {
        let files: Vec<String> = vec!["note.md".into(), "other.md".into()];
        let dirs: Vec<String> = vec!["note".into()];
        let st = stats(0, 2);
        let inp = input("notes", &files, &dirs, &st);
        assert_eq!(classify_by_file_fingerprint(&inp), Some(DirType::NoteCollection));
    }

    #[test]
    fn test_fingerprint_note_collection_assets() {
        let files: Vec<String> = vec!["a.md".into(), "b.md".into()];
        let dirs: Vec<String> = vec!["assets".into()];
        let st = stats(0, 2);
        let inp = input("blog", &files, &dirs, &st);
        assert_eq!(classify_by_file_fingerprint(&inp), Some(DirType::NoteCollection));
    }

    #[test]
    fn test_fingerprint_yaml_library() {
        let files: Vec<String> = vec![
            "cve-2024-1.yaml".into(),
            "cve-2024-2.yaml".into(),
            "cve-2024-3.yml".into(),
            "readme.md".into(),
        ];
        let st = stats(0, 4);
        let inp = input("nuclei-templates", &files, &[], &st);
        assert_eq!(classify_by_file_fingerprint(&inp), Some(DirType::YamlLibrary));
    }

    #[test]
    fn test_fingerprint_doc_collection() {
        let files: Vec<String> = vec![
            "lecture1.pptx".into(),
            "lecture2.pptx".into(),
            "lecture3.pptx".into(),
            "notes.docx".into(),
        ];
        let st = stats(0, 4);
        let inp = input("course", &files, &[], &st);
        assert_eq!(classify_by_file_fingerprint(&inp), Some(DirType::DocCollection));
    }

    #[test]
    fn test_fingerprint_rejects_when_exec_present() {
        // 含 exe 时所有 require_no_exec 类型都不命中
        let files: Vec<String> = vec!["a.md".into(), "b.md".into(), "app.exe".into()];
        let st = stats(1, 3);
        let inp = input("mixed", &files, &[], &st);
        assert_eq!(classify_by_file_fingerprint(&inp), None);
    }

    #[test]
    fn test_fingerprint_no_match_returns_none() {
        let files: Vec<String> = vec!["report.pdf".into(), "data.csv".into()];
        let st = stats(0, 2);
        let inp = input("misc", &files, &[], &st);
        assert_eq!(classify_by_file_fingerprint(&inp), None);
    }

    // ── classify_dir_type 集成（patterns）──

    fn make_pattern(name: &str, dt: &str, keywords: &[&str], markers: &[&str], priority: i32) -> DirPatternRow {
        make_pattern_exec(name, dt, keywords, markers, priority, true)
    }

    /// make_pattern 的变体，可指定 require_no_exec
    fn make_pattern_exec(
        name: &str,
        dt: &str,
        keywords: &[&str],
        markers: &[&str],
        priority: i32,
        require_no_exec: bool,
    ) -> DirPatternRow {
        DirPatternRow {
            id: 0,
            pattern_name: name.into(),
            dir_type: dt.into(),
            dir_name_keywords: keywords.iter().map(|s| s.to_string()).collect(),
            file_markers: markers.iter().map(|s| s.to_string()).collect(),
            file_type_ratio: serde_json::json!({}),
            same_name_dir: false,
            require_no_exec,
            action: "keep".into(),
            target_path: String::new(),
            priority,
            enabled: true,
        }
    }

    // ── require_no_exec 行为测试 ──

    #[test]
    fn test_require_no_exec_keywords_rejects_when_exec_present() {
        // CTF 题目（keywords 命中）+ require_no_exec=true + 含 exe → 不命中，下落到 APP_DIR
        let patterns = vec![
            make_pattern_exec("CTF题目", "CTF_CHALLENGE", &["CTF", "靶场"], &[], 15, true),
        ];
        let files: Vec<String> = vec!["readme.txt".into(), "solver.exe".into()];
        let st = stats(1, 2); // 1 个 exe
        let inp = input("2024靶场", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/ctf/2024靶场"), &inp, &patterns);
        assert_eq!(dt, DirType::AppDir, "含 exe 的 CTF 目录应下落到 APP_DIR");
    }

    #[test]
    fn test_require_no_exec_keywords_passes_without_exec() {
        // CTF 题目（keywords 命中）+ require_no_exec=true + 无 exe → 正常命中
        let patterns = vec![
            make_pattern_exec("CTF题目", "CTF_CHALLENGE", &["CTF", "靶场"], &[], 15, true),
        ];
        let files: Vec<String> = vec!["chall.py".into(), "readme.md".into()];
        let st = stats(0, 2);
        let inp = input("2024靶场", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/ctf/2024靶场"), &inp, &patterns);
        assert_eq!(dt, DirType::CtfChallenge);
    }

    #[test]
    fn test_code_project_allows_exec() {
        // 代码项目（file_markers 命中 go.mod）+ require_no_exec=false + 含 exe → 仍判为 CODE_PROJECT
        // （go build 产物在同目录是常态）
        let patterns = vec![
            make_pattern_exec("代码项目", "CODE_PROJECT", &[], &["go.mod", "package.json"], 10, false),
        ];
        let files: Vec<String> = vec!["go.mod".into(), "main.go".into(), "myapp.exe".into()];
        let st = stats(1, 3); // 1 个 exe
        let inp = input("myapp", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/proj/myapp"), &inp, &patterns);
        assert_eq!(dt, DirType::CodeProject, "代码项目带编译产物应仍判为 CODE_PROJECT");
    }

    #[test]
    fn test_require_no_exec_markers_rejects_when_exec_present() {
        // 即便 file_markers 命中，require_no_exec=true + 含 exe 也不命中
        let patterns = vec![
            make_pattern_exec("笔记", "NOTE_COLLECTION", &[], &["note.md"], 10, true),
        ];
        let files: Vec<String> = vec!["note.md".into(), "index.exe".into()];
        let st = stats(1, 2);
        let inp = input("notes", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/notes"), &inp, &patterns);
        assert_eq!(dt, DirType::AppDir, "含 exe 的笔记目录应下落到 APP_DIR");
    }

    #[test]
    fn test_require_no_exec_falls_through_to_next_pattern() {
        // require_no_exec=true 的 pattern 不命中后，应继续匹配下一条（而非直接 Unknown）
        let patterns = vec![
            make_pattern_exec("笔记", "NOTE_COLLECTION", &["notes"], &[], 10, true),
            make_pattern_exec("代码项目", "CODE_PROJECT", &[], &["go.mod"], 20, false),
        ];
        // 目录名 "notes" 命中第1条，但含 exe → 跳过；go.mod 命中第2条（允许 exe）→ CODE_PROJECT
        let files: Vec<String> = vec!["go.mod".into(), "notes.exe".into()];
        let st = stats(1, 2);
        let inp = input("notes", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/notes"), &inp, &patterns);
        assert_eq!(dt, DirType::CodeProject, "第1条跳过后应继续命中第2条");
    }

    #[test]
    fn test_classify_file_markers_highest_priority() {
        let patterns = vec![
            make_pattern("代码项目", "CODE_PROJECT", &[], &["package.json", "go.mod"], 10),
        ];
        let files: Vec<String> = vec!["package.json".into(), "index.js".into()];
        let st = stats(0, 2);
        let inp = input("myapp", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/proj/myapp"), &inp, &patterns);
        assert_eq!(dt, DirType::CodeProject);
    }

    #[test]
    fn test_classify_dir_name_keyword() {
        let patterns = vec![
            make_pattern("CTF题目", "CTF_CHALLENGE", &["CTF", "数字中国", "攻防"], &[], 15),
        ];
        let files: Vec<String> = vec!["readme.txt".into()];
        let st = stats(0, 1);
        let inp = input("files", &files, &[], &st);
        // 目录名含 "数字中国"
        let dt = classify_dir_type(Path::new("D:/ctf/2024数字中国"), &inp, &patterns);
        assert_eq!(dt, DirType::CtfChallenge);
    }

    #[test]
    fn test_classify_falls_through_to_fingerprint() {
        // 无 pattern 命中 → 走指纹 → YAML_LIBRARY
        let patterns: Vec<DirPatternRow> = vec![];
        let files: Vec<String> = vec!["a.yaml".into(), "b.yaml".into(), "c.yml".into()];
        let st = stats(0, 3);
        let inp = input("pocs", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/pocs"), &inp, &patterns);
        assert_eq!(dt, DirType::YamlLibrary);
    }

    #[test]
    fn test_classify_app_dir_when_exec_present() {
        // 含 exe 且无 pattern/指纹命中 → APP_DIR（交给评分模型）
        let patterns: Vec<DirPatternRow> = vec![];
        let files: Vec<String> = vec!["app.exe".into(), "data.dat".into()];
        let st = stats(1, 2);
        let inp = input("someapp", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/someapp"), &inp, &patterns);
        assert_eq!(dt, DirType::AppDir);
    }

    #[test]
    fn test_classify_unknown_when_nothing_matches() {
        let patterns: Vec<DirPatternRow> = vec![];
        let files: Vec<String> = vec!["report.pdf".into(), "data.csv".into()];
        let st = stats(0, 2);
        let inp = input("misc", &files, &[], &st);
        let dt = classify_dir_type(Path::new("D:/misc"), &inp, &patterns);
        assert_eq!(dt, DirType::Unknown);
    }
}
