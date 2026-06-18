use crate::core::models::FileRecord;
use regex::Regex;
use std::cmp::Ordering;
use std::sync::LazyLock;

static SEMVER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-_\s]v?(\d+\.\d+\.\d+)").unwrap());
static SIMPLE_VER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-_\s]v?(\d+\.\d+)").unwrap());
static DATE_VER_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-_\s](\d{8})").unwrap());
static BUILD_NUM_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[-_\s](\d+)$").unwrap());

/// Extract version from filename with priority: semver > simple > date > build number
pub fn extract_version(filename: &str) -> (String, bool) {
    let name = strip_extension(filename);

    if let Some(caps) = SEMVER_RE.captures(&name) {
        return (caps[1].to_string(), true);
    }
    if let Some(caps) = SIMPLE_VER_RE.captures(&name) {
        return (caps[1].to_string(), true);
    }
    if let Some(caps) = DATE_VER_RE.captures(&name) {
        return (caps[1].to_string(), true);
    }
    if let Some(caps) = BUILD_NUM_RE.captures(&name) {
        return (caps[1].to_string(), true);
    }
    (String::new(), false)
}

/// Strip extension from a filename, handling compound extensions like .tar.gz
pub fn strip_extension(name: &str) -> String {
    let name_lower = name.to_lowercase();
    for ext in &[".tar.gz", ".tar.xz", ".tar.bz2"] {
        if name_lower.ends_with(ext) {
            return name[..name.len() - ext.len()].to_string();
        }
    }
    if let Some(idx) = name.rfind('.') {
        name[..idx].to_string()
    } else {
        name.to_string()
    }
}

fn is_date_version(s: &str) -> bool {
    s.len() == 8 && s.chars().all(|c| c.is_ascii_digit())
}

/// Compare two version strings. Returns Ordering.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    if is_date_version(a) && is_date_version(b) {
        let ai: i64 = a.parse().unwrap_or(0);
        let bi: i64 = b.parse().unwrap_or(0);
        return ai.cmp(&bi);
    }

    let parse_parts = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };

    let pa = parse_parts(a);
    let pb = parse_parts(b);

    for i in 0..pa.len().max(pb.len()) {
        let va = pa.get(i).unwrap_or(&0);
        let vb = pb.get(i).unwrap_or(&0);
        match va.cmp(vb) {
            Ordering::Equal => continue,
            other => return other,
        }
    }
    Ordering::Equal
}

/// Find the latest file from a list of FileRecords based on version or mod time
pub fn find_latest(files: &[FileRecord]) -> Option<FileRecord> {
    if files.is_empty() {
        return None;
    }

    let mut versioned: Vec<(&FileRecord, String)> = Vec::new();
    let mut unversioned: Vec<&FileRecord> = Vec::new();

    for f in files {
        if let (ver, true) = extract_version(&f.name) {
            versioned.push((f, ver));
        } else {
            unversioned.push(f);
        }
    }

    if !versioned.is_empty() {
        let mut best = versioned[0].clone();
        for vf in &versioned[1..] {
            if compare_versions(&vf.1, &best.1) == Ordering::Greater {
                best = vf.clone();
            } else if compare_versions(&vf.1, &best.1) == Ordering::Equal {
                if vf.0.mod_time > best.0.mod_time {
                    best = vf.clone();
                }
            }
        }
        return Some(best.0.clone());
    }

    let mut best = unversioned[0];
    for f in &unversioned[1..] {
        if f.mod_time > best.mod_time {
            best = f;
        }
    }
    Some(best.clone())
}

/// Compute Levenshtein distance between two strings
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let la = a.chars().count();
    let lb = b.chars().count();

    if la == 0 {
        return lb;
    }
    if lb == 0 {
        return la;
    }

    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let mut prev = vec![0usize; lb + 1];
    let mut curr = vec![0usize; lb + 1];

    for j in 0..=lb {
        prev[j] = j;
    }

    for i in 1..=la {
        curr[0] = i;
        for j in 1..=lb {
            let cost = if a_chars[i - 1] == b_chars[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[lb]
}
