use crate::core::models::{DedupGroup, FileRecord};
use crate::core::version::{compare_versions, extract_version, levenshtein_distance};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

static VERSION_STRIP_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-_\s]v?\d[\d.]*").unwrap());

pub struct DedupDetector {
    pub prefer_uncompressed: bool,
    pub fuzzy_threshold: usize,
}

impl DedupDetector {
    pub fn new(prefer_uncompressed: bool, fuzzy_threshold: usize) -> Self {
        Self {
            prefer_uncompressed,
            fuzzy_threshold: if fuzzy_threshold > 0 { fuzzy_threshold } else { 2 },
        }
    }

    pub fn detect(&self, records: &[FileRecord]) -> Vec<DedupGroup> {
        let mut used: HashSet<usize> = HashSet::new();
        let mut groups = Vec::new();

        groups.extend(self.exact_hash_match(records, &mut used));
        groups.extend(self.version_group_match(records, &mut used));
        groups.extend(self.redundant_archive_match(records, &mut used));
        groups.extend(self.size_match(records, &mut used));
        groups.extend(self.fuzzy_name_match(records, &mut used));

        groups
    }

    fn exact_hash_match(&self, records: &[FileRecord], used: &mut HashSet<usize>) -> Vec<DedupGroup> {
        let mut by_hash: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, r) in records.iter().enumerate() {
            if used.contains(&i) || r.is_app_dir {
                continue;
            }
            by_hash.entry(r.file_hash.clone()).or_default().push(i);
        }

        let mut groups = Vec::new();
        for indices in by_hash.values() {
            if indices.len() < 2 {
                continue;
            }
            let rep_idx = self.select_representative(records, indices);
            let mut duplicates = Vec::new();
            for &idx in indices {
                used.insert(idx);
                if idx != rep_idx {
                    duplicates.push(records[idx].clone());
                }
            }
            if !duplicates.is_empty() {
                groups.push(DedupGroup {
                    representative: records[rep_idx].clone(),
                    duplicates,
                    reason: "hash_match".to_string(),
                });
            }
        }
        groups
    }

    fn version_group_match(&self, records: &[FileRecord], used: &mut HashSet<usize>) -> Vec<DedupGroup> {
        struct Keyed {
            idx: usize,
            base: String,
            ver: String,
        }
        let mut versioned = Vec::new();
        for (i, r) in records.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }
            let (ver, ok) = extract_version(&r.name);
            if !ok || ver.is_empty() {
                continue;
            }
            let base = extract_base_name(&r.name);
            if base.is_empty() {
                continue;
            }
            versioned.push(Keyed { idx: i, base, ver });
        }

        let mut by_base: HashMap<String, Vec<&Keyed>> = HashMap::new();
        for v in &versioned {
            let ext = if let Some(e) = Path::new(&records[v.idx].name).extension() {
                format!(".{}", e.to_string_lossy())
            } else {
                String::new()
            };
            let key = format!("{}{}", v.base, ext);
            by_base.entry(key).or_default().push(v);
        }

        let mut groups = Vec::new();
        for items in by_base.values() {
            if items.len() < 2 {
                continue;
            }
            let indices: Vec<usize> = items.iter().map(|v| v.idx).collect();
            let rep_idx = self.select_representative(records, &indices);
            let mut duplicates = Vec::new();
            for &idx in &indices {
                used.insert(idx);
                if idx != rep_idx {
                    duplicates.push(records[idx].clone());
                }
            }
            if !duplicates.is_empty() {
                groups.push(DedupGroup {
                    representative: records[rep_idx].clone(),
                    duplicates,
                    reason: "multi_version".to_string(),
                });
            }
        }
        groups
    }

    fn redundant_archive_match(&self, records: &[FileRecord], used: &mut HashSet<usize>) -> Vec<DedupGroup> {
        let archive_exts: HashSet<&str> =
            [".zip", ".7z", ".rar", ".gz", ".tar", ".xz", ".bz2"].iter().copied().collect();

        struct Entry {
            idx: usize,
            normalized: String,
        }

        let mut archives = Vec::new();
        let mut non_archives = Vec::new();

        for (i, r) in records.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }
            let ext_lower = r.extension.to_lowercase();
            let name_lower = r.name.to_lowercase();
            let is_archive = archive_exts.contains(ext_lower.as_str())
                || name_lower.ends_with(".tar.gz")
                || name_lower.ends_with(".tar.xz")
                || name_lower.ends_with(".tar.bz2");

            let norm = normalize_for_archive_match(&r.name);
            if norm.is_empty() {
                continue;
            }

            if is_archive {
                archives.push(Entry { idx: i, normalized: norm });
            } else {
                non_archives.push(Entry { idx: i, normalized: norm });
            }
        }

        let mut groups = Vec::new();
        let mut used_archive: HashSet<usize> = HashSet::new();

        for arch in &archives {
            if used_archive.contains(&arch.idx) {
                continue;
            }
            for non_arch in &non_archives {
                if arch.normalized == non_arch.normalized {
                    let rep_idx = non_arch.idx;
                    let dup_idx = arch.idx;
                    used.insert(dup_idx);
                    used_archive.insert(arch.idx);
                    groups.push(DedupGroup {
                        representative: records[rep_idx].clone(),
                        duplicates: vec![records[dup_idx].clone()],
                        reason: "redundant_archive".to_string(),
                    });
                    break;
                }
            }
        }
        groups
    }

    fn size_match(&self, records: &[FileRecord], used: &mut HashSet<usize>) -> Vec<DedupGroup> {
        let mut by_size: HashMap<i64, Vec<usize>> = HashMap::new();
        for (i, r) in records.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }
            by_size.entry(r.file_size).or_default().push(i);
        }

        let mut groups = Vec::new();
        for indices in by_size.values() {
            if indices.len() < 2 {
                continue;
            }
            let related = self.find_fuzzy_related(records, indices);
            for group in related {
                if group.len() < 2 {
                    continue;
                }
                let rep_idx = self.select_representative(records, &group);
                let mut duplicates = Vec::new();
                for &idx in &group {
                    used.insert(idx);
                    if idx != rep_idx {
                        duplicates.push(records[idx].clone());
                    }
                }
                if !duplicates.is_empty() {
                    groups.push(DedupGroup {
                        representative: records[rep_idx].clone(),
                        duplicates,
                        reason: "size_only".to_string(),
                    });
                }
            }
        }
        groups
    }

    fn find_fuzzy_related(&self, records: &[FileRecord], indices: &[usize]) -> Vec<Vec<usize>> {
        let mut matched: HashSet<usize> = HashSet::new();
        let mut groups = Vec::new();

        for (i, &idx_a) in indices.iter().enumerate() {
            if matched.contains(&idx_a) {
                continue;
            }
            let mut group = vec![idx_a];
            matched.insert(idx_a);

            for &idx_b in &indices[i + 1..] {
                if matched.contains(&idx_b) {
                    continue;
                }
                let name_a = normalize_name(&records[idx_a].name);
                let name_b = normalize_name(&records[idx_b].name);
                if levenshtein_distance(&name_a, &name_b) <= self.fuzzy_threshold {
                    group.push(idx_b);
                    matched.insert(idx_b);
                }
            }
            groups.push(group);
        }
        groups
    }

    fn fuzzy_name_match(&self, records: &[FileRecord], used: &mut HashSet<usize>) -> Vec<DedupGroup> {
        let unused: Vec<usize> = (0..records.len()).filter(|i| !used.contains(i)).collect();
        let mut groups = Vec::new();
        let mut matched: HashSet<usize> = HashSet::new();

        for (i, &idx_a) in unused.iter().enumerate() {
            if matched.contains(&idx_a) {
                continue;
            }
            let mut group = vec![idx_a];

            for &idx_b in &unused[i + 1..] {
                if matched.contains(&idx_b) {
                    continue;
                }
                let name_a = normalize_name(&records[idx_a].name);
                let name_b = normalize_name(&records[idx_b].name);
                if levenshtein_distance(&name_a, &name_b) <= self.fuzzy_threshold {
                    group.push(idx_b);
                }
            }

            if group.len() >= 2 {
                let rep_idx = self.select_representative(records, &group);
                let mut duplicates = Vec::new();
                for &idx in &group {
                    matched.insert(idx);
                    used.insert(idx);
                    if idx != rep_idx {
                        duplicates.push(records[idx].clone());
                    }
                }
                if !duplicates.is_empty() {
                    groups.push(DedupGroup {
                        representative: records[rep_idx].clone(),
                        duplicates,
                        reason: "fuzzy_name".to_string(),
                    });
                }
            }
        }
        groups
    }

    fn select_representative(&self, records: &[FileRecord], indices: &[usize]) -> usize {
        let mut best = indices[0];
        for &idx in &indices[1..] {
            if self.is_better_representative(&records[idx], &records[best]) {
                best = idx;
            }
        }
        best
    }

    fn is_better_representative(&self, a: &FileRecord, b: &FileRecord) -> bool {
        if self.prefer_uncompressed {
            let a_unc = is_uncompressed(&a.extension);
            let b_unc = is_uncompressed(&b.extension);
            if a_unc && !b_unc {
                return true;
            }
            if !a_unc && b_unc {
                return false;
            }
        }

        let (a_ver, a_ok) = extract_version(&a.name);
        let (b_ver, b_ok) = extract_version(&b.name);
        if a_ok && b_ok {
            match compare_versions(&a_ver, &b_ver) {
                Ordering::Greater => return true,
                Ordering::Less => return false,
                _ => {}
            }
        }
        if a_ok && !b_ok {
            return true;
        }
        if !a_ok && b_ok {
            return false;
        }

        a.mod_time > b.mod_time
    }
}

fn is_uncompressed(ext: &str) -> bool {
    matches!(
        ext.to_lowercase().as_str(),
        ".exe" | ".msi" | ".pkg" | ".dmg" | ".deb" | ".rpm" | ".jar" | ".appimage"
    )
}

/// Extract base name by stripping version, platform, arch, and date suffixes
pub fn extract_base_name(filename: &str) -> String {
    let name = crate::core::version::strip_extension(filename);
    if let Some(loc) = VERSION_STRIP_RE.find(&name) {
        name[..loc.start()].to_string()
    } else {
        name
    }
    .trim()
    .to_lowercase()
}

pub fn normalize_name(name: &str) -> String {
    let mut base = name.to_string();
    let ext = Path::new(name)
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    if !ext.is_empty() {
        base = base[..base.len() - ext.len()].to_string();
    }
    base = base.to_lowercase();
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

fn normalize_for_archive_match(name: &str) -> String {
    let mut base = name.to_string();
    let ext = Path::new(name)
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
        "setup", "install", "installer", "update",
        "win64", "win32", "windowsamd64", "windowsx64", "windowsx86",
        "amd64", "x64", "x86", "64bit", "32bit",
        "linuxamd64", "linuxx64", "darwinamd64", "darwinx64",
    ] {
        if base.ends_with(suffix) {
            base = base[..base.len() - suffix.len()].to_string();
        }
    }
    base
}
