pub use crate::core::models::AppDirSignature;
use crate::core::version::{extract_version, levenshtein_distance};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

pub fn detect_app_dir(dir_path: &Path) -> AppDirSignature {
    let entries = match fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(_) => return AppDirSignature::default(),
    };

    let mut exes: Vec<String> = Vec::new();
    let mut dlls: Vec<String> = Vec::new();
    let mut has_doc = false;

    for entry in entries.flatten() {
        if entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        let name_lower = name.to_lowercase();
        let ext = name_lower
            .rsplit('.')
            .next()
            .unwrap_or("")
            .to_string();

        match ext.as_str() {
            "exe" => {
                if !is_noise_exe(&name_lower) {
                    exes.push(name);
                }
            }
            "dll" => {
                dlls.push(name);
            }
            _ => {
                if !has_doc && is_doc_file(&name_lower) {
                    has_doc = true;
                }
            }
        }
    }

    let dir_base = dir_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // R1: >=1 exe + >=3 dll => confidence 0.90
    if exes.len() >= 1 && dlls.len() >= 3 {
        let main_exe = pick_main_exe(&exes, &dir_base);
        return AppDirSignature {
            is_app_dir: true,
            main_exe,
            app_name: infer_app_name(&dir_base),
            confidence: 0.90,
            reason: "exe+dll".to_string(),
        };
    }

    // R2: >=1 exe + doc => confidence 0.80
    if exes.len() >= 1 && has_doc {
        let main_exe = pick_main_exe(&exes, &dir_base);
        return AppDirSignature {
            is_app_dir: true,
            main_exe,
            app_name: infer_app_name(&dir_base),
            confidence: 0.80,
            reason: "exe+doc".to_string(),
        };
    }

    // R3: exactly 1 exe + 1~2 dll => confidence 0.70
    if exes.len() == 1 && dlls.len() >= 1 && dlls.len() <= 2 {
        return AppDirSignature {
            is_app_dir: true,
            main_exe: exes[0].clone(),
            app_name: infer_app_name(&dir_base),
            confidence: 0.70,
            reason: "single-exe+dll".to_string(),
        };
    }

    AppDirSignature::default()
}

pub fn infer_app_name(dir_base: &str) -> String {
    let (ver, ok) = extract_version(dir_base);
    if !ok || ver.is_empty() {
        return dir_base.to_string();
    }
    if let Some(idx) = dir_base.find(&ver) {
        if idx > 0 {
            let mut name = dir_base[..idx].to_string();
            name = name.trim_end_matches(|c: char| c == '-' || c == '_' || c == ' ' || c == 'v' || c == 'V' || c == '.').to_string();
            if name.is_empty() {
                return dir_base.to_string();
            }
            return name;
        }
    }
    dir_base.to_string()
}

pub fn pick_main_exe(candidates: &[String], dir_name: &str) -> String {
    if candidates.len() == 1 {
        return candidates[0].clone();
    }
    let dir_norm = normalize_for_pick(&dir_name.to_lowercase().replace(' ', ""));

    let mut best = candidates[0].clone();
    let best_stem = best.trim_end_matches(".exe").to_lowercase();
    let mut best_dist = levenshtein_distance(&dir_norm, &normalize_for_pick(&best_stem));

    for c in &candidates[1..] {
        let c_stem = c.trim_end_matches(".exe").to_lowercase();
        let d = levenshtein_distance(&dir_norm, &normalize_for_pick(&c_stem));
        if d < best_dist {
            best_dist = d;
            best = c.clone();
        }
    }
    best
}

fn normalize_for_pick(s: &str) -> String {
    s.replace(' ', "").replace('-', "").replace('_', "")
}

fn is_noise_exe(name_lower: &str) -> bool {
    let prefixes = [
        "unin", "unins", "uninst", "uninstall",
        "helper", "updater", "update",
        "crashreport", "crash_report",
        "setup", "install",
        "registrator", "register",
        "elevate", "launcher_helper",
    ];
    prefixes.iter().any(|p| name_lower.starts_with(p))
}

fn is_doc_file(name_lower: &str) -> bool {
    const DOC_FILES: &[&str] = &[
        "readme.txt", "readme.md", "readme",
        "license.txt", "license.md", "licence.txt",
        "release.txt", "release_notes.txt",
        "changelog.txt", "changes.txt",
        "说明.txt", "使用说明.txt", "使用说明.md", "说明书.txt",
        "帮助.txt", "帮助文档.txt", "版本说明.txt", "更新日志.txt",
        "readme_zh.txt", "readme_cn.txt",
    ];
    DOC_FILES.contains(&name_lower)
}

pub fn compute_dir_hash(dir_path: &str, exe_names: &[String]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(dir_path.as_bytes());
    hasher.update(b"|");
    hasher.update(exe_names.join(",").as_bytes());
    hex::encode(hasher.finalize())
}

pub fn compute_dir_size(dir_path: &Path) -> i64 {
    let mut total: i64 = 0;
    for entry in walkdir::WalkDir::new(dir_path).into_iter() {
        if let Ok(entry) = entry {
            if entry.file_type().is_file() {
                if let Ok(metadata) = entry.metadata() {
                    total += metadata.len() as i64;
                }
            }
        }
    }
    total
}

// ────────────────── v2: 可执行标记 + 子树统计 ──────────────────

/// 可执行文件后缀（触发 app dir 判定的标记）
pub fn is_executable_marker(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    const EXTS: &[&str] = &[".exe", ".jar", ".app", ".bat", ".cmd"];
    EXTS.iter().any(|e| lower.ends_with(e))
}

/// Python 项目辅助文件白名单（用于 Python 项目占比判定）
fn is_python_aux_file(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    const EXTS: &[&str] = &[
        ".py", ".pyw", ".txt", ".md", ".cfg", ".toml", ".json", ".rst", ".ini", ".yaml", ".yml",
    ];
    EXTS.iter().any(|e| lower.ends_with(e))
}

/// 判断文件名是否为 .py
fn is_python_file(file_name: &str) -> bool {
    file_name.to_lowercase().ends_with(".py")
}

/// 目录子树的文件统计（用于自底向上标记）
#[derive(Debug, Clone, Default)]
pub struct SubtreeStats {
    pub has_exec: bool,
    pub py_count: usize,
    pub aux_count: usize,
    pub total_files: usize,
}

impl SubtreeStats {
    /// 合并子目录的统计到当前
    pub fn merge_child(&mut self, child: &SubtreeStats) {
        self.has_exec = self.has_exec || child.has_exec;
        self.py_count += child.py_count;
        self.aux_count += child.aux_count;
        self.total_files += child.total_files;
    }

    /// 判定是否为 Python 工具项目子树（无 exe + 有 .py + 占比 ≥80%）
    pub fn is_python_project(&self) -> bool {
        if self.has_exec || self.py_count == 0 || self.total_files == 0 {
            return false;
        }
        (self.aux_count * 100) / self.total_files >= 80
    }

    /// 子树是否应作为 app dir 候选（含可执行 或 Python 项目）
    pub fn is_app_candidate(&self) -> bool {
        self.has_exec || self.is_python_project()
    }
}

/// 从文件名得到统计贡献 (is_exec, is_py, is_aux)
pub fn stats_for_file(file_name: &str) -> (bool, bool, bool) {
    let is_exec = is_executable_marker(file_name);
    let is_py = is_python_file(file_name);
    let is_aux = is_python_aux_file(file_name);
    (is_exec, is_py, is_aux)
}

/// 收集目录子树下所有可执行文件的相对路径（相对于 base，用 / 分隔）
pub fn collect_executables_in_subtree(base: &Path) -> Vec<String> {
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(base).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_executable_marker(&name) {
            if let Ok(rel) = entry.path().strip_prefix(base) {
                result.push(rel.to_string_lossy().replace('\\', "/"));
            } else {
                result.push(name);
            }
        }
    }
    result
}

