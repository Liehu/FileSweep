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

fn pick_main_exe(candidates: &[String], dir_name: &str) -> String {
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
