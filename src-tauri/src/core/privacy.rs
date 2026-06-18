use crate::core::models::FileRecord;
use std::path::Path;

pub struct PrivacyChecker {
    pub rules: Vec<String>,
}

impl PrivacyChecker {
    pub fn new(rules: Vec<String>) -> Self {
        Self { rules }
    }

    pub fn should_skip(&self, file: &FileRecord) -> bool {
        if file.ai_skip {
            return true;
        }
        if self.match_patterns(&file.name, &file.local_path) {
            return true;
        }
        // Match parent directory
        if let Some(parent) = Path::new(&file.local_path).parent() {
            if self.match_patterns("", &parent.to_string_lossy()) {
                return true;
            }
        }
        false
    }

    fn match_patterns(&self, name: &str, path: &str) -> bool {
        for pattern in &self.rules {
            if match_pattern(pattern, name) || match_pattern(pattern, path) {
                return true;
            }
        }
        false
    }
}

fn match_pattern(pattern: &str, s: &str) -> bool {
    let lower = s.to_lowercase();
    let p = pattern.to_lowercase();

    if p.contains('*') || p.contains('?') {
        glob_match(&p, &lower)
    } else {
        lower.contains(&p)
    }
}

fn glob_match(pattern: &str, s: &str) -> bool {
    if pattern.contains("**") {
        return glob_match_double_star(pattern, s);
    }

    let mut px = 0;
    let mut sx = 0;
    let mut star_idx = None;
    let mut match_idx = 0;
    let p: Vec<char> = pattern.chars().collect();
    let st: Vec<char> = s.chars().collect();

    while sx < st.len() {
        if px < p.len() && (p[px] == st[sx] || p[px] == '?') {
            px += 1;
            sx += 1;
        } else if px < p.len() && p[px] == '*' {
            star_idx = Some(px);
            match_idx = sx;
            px += 1;
        } else if let Some(si) = star_idx {
            px = si + 1;
            match_idx += 1;
            sx = match_idx;
        } else {
            return false;
        }
    }

    while px < p.len() && p[px] == '*' {
        px += 1;
    }

    px == p.len()
}

fn glob_match_double_star(pattern: &str, s: &str) -> bool {
    let segments: Vec<&str> = pattern.split("**").collect();
    if segments.len() == 1 {
        return glob_match(pattern, s);
    }
    match_double_star_segments(&segments, s, 0, 0)
}

fn match_double_star_segments(segments: &[&str], s: &str, seg_idx: usize, str_idx: usize) -> bool {
    if seg_idx >= segments.len() {
        return str_idx >= s.len();
    }

    let seg = segments[seg_idx];
    let is_last = seg_idx == segments.len() - 1;

    for i in str_idx..=s.len() {
        if glob_match(seg, &s[str_idx..i]) {
            if is_last {
                if i == s.len() {
                    return true;
                }
            } else if match_double_star_segments(segments, s, seg_idx + 1, i) {
                return true;
            }
        }
    }
    false
}
