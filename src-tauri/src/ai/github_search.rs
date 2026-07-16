//! GitHub 搜索增强 AI 丰富准确性。
//!
//! 思路：文件大多是 GitHub 下载的原始名（用户不改名），先搜 GitHub 拿到仓库事实
//! （full_name/description/stars/topics），塞进 enrich prompt 作为"已知事实"，
//! AI 基于事实做功能分类/描述，准确性大幅提升。
//!
//! 约束：GitHub Search API 认证 30 req/min、未认证 10 req/min。需速率控制 + 建议填 token。
//! 哈希文件名搜不到 → 跳过，走原 enrich（不退化）。

use std::time::Duration;

/// GitHub 搜索命中的仓库信息，作为 enrich 的"已知事实"提示给 LLM。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct GitHubHint {
    /// "owner/repo" 全名，如 "MountCloud/BehinderClientSource"
    pub full_name: String,
    /// 仓库名（不含 owner），用于评分匹配
    pub name: String,
    /// 仓库描述（README 首行），给 LLM 当事实
    #[serde(default)]
    pub description: String,
    /// 仓库主页 URL
    pub html_url: String,
    /// star 数（评分权重 + 给 LLM 判断知名度）
    pub stars: i64,
    /// 仓库 topics 标签，帮助 LLM 分类
    #[serde(default)]
    pub topics: Vec<String>,
    /// 项目主页（homepage，可能为空）
    #[serde(default)]
    pub homepage: String,
}

/// 内部候选（解析自 GitHub API items）
#[derive(Debug, Clone)]
struct RepoCandidate {
    full_name: String,
    name: String,
    description: String,
    stars: i64,
    html_url: String,
    topics: Vec<String>,
    homepage: String,
}

// ────────────────── 文件名归一化 ──────────────────

/// 归一化文件名用于 GitHub 搜索。
///
/// **只去**浏览器/系统下载重名时加的重复后缀 ` (1)` ` (2)` `（1）`（中英文括号、空格变体）。
/// **不去**版本号/平台后缀/分支名——用户不改名，原文在 GitHub 上匹配最精确。
///
/// 例：`BeeCount-main (1).zip` → `BeeCount-main.zip`；`Directory Opus 13.19.zip` → 原样。
pub fn normalize_filename_for_search(name: &str) -> String {
    // 去重复后缀： (1) / (2) / （1）（中文括号），允许前后空格
    // 用正则去掉所有形如 " (N)" 的片段
    let mut result = name.to_string();
    // 英文括号 + 数字： " (1)" / "(2)"
    result = regex_simple_dedup(&result);
    result.trim().to_string()
}

/// 简单去重复后缀：手写实现避免引入 regex 依赖。
/// 去掉 " (1)" / " (2)" / "（1）" 等模式（中英文括号、可选空格、1-3 位数字）。
fn regex_simple_dedup(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        // 检测 " (N)" 或 "（N）" 模式：可选空格 + ( 或 （ + 数字 + ) 或 ）
        let c = chars[i];
        let is_open = c == '(' || c == '（';
        let prev_space = i > 0 && (chars[i - 1] == ' ' || chars[i - 1] == '\u{a0}');
        if is_open {
            // 找到闭合括号位置
            let close_idx = chars[i..].iter().position(|&x| x == ')' || x == '）');
            if let Some(rel_close) = close_idx {
                let inner: String = chars[i + 1..i + rel_close].iter().collect();
                let inner_trim = inner.trim();
                // 内部是纯数字（1-3 位）→ 视为重复后缀，跳过整段（含前导空格）
                if !inner_trim.is_empty()
                    && inner_trim.chars().all(|x| x.is_ascii_digit())
                    && inner_trim.len() <= 3
                {
                    // 去掉前面的空格（若有）
                    while out.ends_with(' ') || out.ends_with('\u{a0}') {
                        out.pop();
                    }
                    i = i + rel_close + 1;
                    continue;
                }
            }
        }
        let _ = prev_space; // 占位，逻辑已直接处理空格
        out.push(c);
        i += 1;
    }
    out
}

/// 判断归一化后的文件名是否值得搜 GitHub。
///
/// 跳过：纯十六进制 ≥16 字符（哈希文件名，如 `3c805586e5844fa8...zip`）。
/// 这些 GitHub 搜不到，浪费配额。其余都搜。
fn should_search(normalized: &str) -> bool {
    // 去扩展名后看 stem
    let stem = normalized.rsplit_once('.').map(|(s, _)| s).unwrap_or(normalized);
    let stem = stem.trim();
    if stem.is_empty() {
        return false;
    }
    // 纯十六进制且 ≥16 字符 → 哈希名，跳过
    if stem.len() >= 16 && stem.chars().all(|c| c.is_ascii_hexdigit()) {
        return false;
    }
    // 纯数字 → 跳过（如 "123.zip"）
    if stem.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    true
}

// ────────────────── GitHub Search 客户端 ──────────────────

pub struct GitHubSearcher {
    client: reqwest::Client,
    api_token: Option<String>,
}

impl GitHubSearcher {
    pub fn new(api_token: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .user_agent("FileSweep") // GitHub API 强制要求 User-Agent
            .build()
            .unwrap_or_default();
        Self { client, api_token }
    }

    /// 搜索仓库：返回 top-5 候选（按 stars 降序）。
    ///
    /// 限流：429/403 时读 X-RateLimit-Reset 退避重试（最多 3 次）。
    async fn search_repo(&self, query: &str) -> Result<Vec<RepoCandidate>, String> {
        let url = "https://api.github.com/search/repositories";
        let mut last_err = String::new();

        for attempt in 0..3u32 {
            // 中断检查
            if crate::commands::enrich::is_enrich_cancelled() {
                return Err("enrich cancelled".into());
            }

            let mut req = self
                .client
                .get(url)
                .query(&[
                    ("q", query),
                    ("sort", "stars"),
                    ("order", "desc"),
                    ("per_page", "5"),
                ]);
            if let Some(token) = &self.api_token {
                if !token.is_empty() {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }
            }

            let resp = req.send().await.map_err(|e| format!("GitHub 请求失败: {}", e))?;
            let status = resp.status();

            if status.is_success() {
                let json: serde_json::Value =
                    serde_json::from_str(&resp.text().await.unwrap_or_default())
                        .map_err(|e| format!("GitHub 响应解析失败: {}", e))?;
                let items = json
                    .get("items")
                    .and_then(|v| v.as_array())
                    .cloned()
                    .unwrap_or_default();
                let candidates = items
                    .iter()
                    .map(|it| RepoCandidate {
                        full_name: it
                            .get("full_name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: it
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        description: it
                            .get("description")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        stars: it.get("stargazers_count").and_then(|v| v.as_i64()).unwrap_or(0),
                        html_url: it
                            .get("html_url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        topics: it
                            .get("topics")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        homepage: it
                            .get("homepage")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    })
                    .collect();
                return Ok(candidates);
            } else if (status.as_u16() == 429 || status.as_u16() == 403) && attempt < 2 {
                // 限流：读 X-RateLimit-Reset（unix 秒），sleep 到那时
                let reset = resp
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<i64>().ok())
                    .unwrap_or(0);
                let now = chrono::Utc::now().timestamp();
                let wait = if reset > now {
                    (reset - now) as u64
                } else {
                    20 // 兜底等 20s
                };
                // 上限 60s，避免异常值导致长时间阻塞
                let wait = wait.min(60);
                log::warn!(
                    "GitHub 限流 ({}), attempt {}/3, sleep {}s",
                    status.as_u16(),
                    attempt + 1,
                    wait
                );
                last_err = format!("GitHub rate limited ({})", status.as_u16());
                tokio::time::sleep(Duration::from_secs(wait)).await;
                continue;
            } else {
                let body = resp.text().await.unwrap_or_default();
                last_err = format!("GitHub {}: {}", status.as_u16(), body);
                log::warn!("{}", last_err);
                return Err(last_err);
            }
        }
        Err(format!("GitHub 搜索重试耗尽: {}", last_err))
    }

    /// 对文件名做 GitHub 搜索并选最优匹配。
    ///
    /// 流程：归一化 → 跳过判定 → 搜索 → 评分选最优（>0.5 才采纳）。
    /// 无候选/归一化跳过/评分不足 → None（调用方走原 enrich）。
    pub async fn find_best_match(&self, filename: &str) -> Option<GitHubHint> {
        let normalized = normalize_filename_for_search(filename);
        if !should_search(&normalized) {
            return None;
        }
        // 去扩展名作 query（保留版本号/平台后缀，GitHub 智能匹配）
        let query = normalized.rsplit_once('.').map(|(s, _)| s).unwrap_or(&normalized);
        let query = query.trim();
        if query.is_empty() {
            return None;
        }

        let candidates = match self.search_repo(query).await {
            Ok(c) => c,
            Err(e) => {
                log::debug!("GitHub 搜索 '{}' 失败（走原 enrich）: {}", query, e);
                return None;
            }
        };

        // 评分选最优
        let query_lower = query.to_lowercase();
        let best = candidates
            .iter()
            .map(|c| (score_candidate(&query_lower, c), c))
            .filter(|(score, _)| *score > 0.5)
            .max_by(|(s1, _), (s2, _)| s1.partial_cmp(s2).unwrap_or(std::cmp::Ordering::Equal));

        match best {
            Some((score, c)) => {
                log::info!(
                    "GitHub 命中 '{}' → {} ★{} (score {:.2})",
                    query,
                    c.full_name,
                    c.stars,
                    score
                );
                Some(GitHubHint {
                    full_name: c.full_name.clone(),
                    name: c.name.clone(),
                    description: c.description.clone(),
                    html_url: c.html_url.clone(),
                    stars: c.stars,
                    topics: c.topics.clone(),
                    homepage: c.homepage.clone(),
                })
            }
            None => None,
        }
    }
}

// ────────────────── 匹配评分 ──────────────────

/// 评估文件名 query 与 GitHub 候选仓库的匹配度。
///
/// 返回 0.0-1.5 综合分（名字匹配分 + stars 权重）：
/// - query 小写 == repo name 小写 → 完美 1.0
/// - repo name 以 query 开头或反之 → 强 0.7
/// - query 是 repo name 子串或反之 → 中 0.4
/// - 其余（无名字关联）→ 0.0
/// - stars 加成：log10(stars+1)/10（低权重，最多 +0.5 给万星仓库）
///
/// 阈值 0.5：完美/强匹配 + 高 stars 能过；弱关联（如 PixPin → pixpin-manager）被过滤。
fn score_candidate(query_lower: &str, c: &RepoCandidate) -> f64 {
    let name_lower = c.name.to_lowercase();
    let name_match = if name_lower == query_lower {
        // 完美：名字完全相等
        1.0
    } else if query_lower.starts_with(&name_lower) && !name_lower.is_empty() {
        // repo name 是 query 的前缀（query 更长，如 query=easytier-windows... name=easytier）
        // → query 很可能就是这个仓库的发布产物（带平台/版本后缀）→ 强匹配
        0.8
    } else if name_lower.starts_with(query_lower) && !query_lower.is_empty() {
        // query 是 repo name 的前缀（repo name 更长，如 query=pixpin name=pixpin-manager）
        // → 可能是不同工具（加了 -manager/-cloud 等后缀）→ 中匹配，需高 stars 才采纳
        0.4
    } else if name_lower.contains(query_lower) || query_lower.contains(&name_lower) {
        // 子串关系（非前缀）→ 弱
        0.3
    } else {
        // full_name 也算（owner/repo 形式，query 可能匹配 full_name）
        let full_lower = c.full_name.to_lowercase();
        if full_lower.contains(query_lower) {
            0.3
        } else {
            0.0
        }
    };
    // stars 加成：仅当名字有匹配时才计 stars（避免高 star 但名字无关的仓库蒙混过关）。
    // log10(stars+1)/10，万星 ≈ +0.4，百星 ≈ +0.2。
    if name_match == 0.0 {
        0.0
    } else {
        let stars_bonus = (c.stars as f64 + 1.0).log10() / 10.0;
        name_match + stars_bonus
    }
}

// ────────────────── 测试 ──────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_dedup_suffix() {
        // 去重复后缀 (1)/(2)
        assert_eq!(normalize_filename_for_search("BeeCount-main (1).zip"), "BeeCount-main.zip");
        assert_eq!(normalize_filename_for_search("report (2).pdf"), "report.pdf");
        // 中文括号
        assert_eq!(normalize_filename_for_search("工具（1）.zip"), "工具.zip");
        // 多个重复后缀
        assert_eq!(normalize_filename_for_search("a (1) (2).zip"), "a.zip");
        // 无重复后缀 → 原样
        assert_eq!(normalize_filename_for_search("croc_v10.3.1_Windows-64bit.zip"), "croc_v10.3.1_Windows-64bit.zip");
        assert_eq!(normalize_filename_for_search("Directory Opus 13.19.zip"), "Directory Opus 13.19.zip");
    }

    #[test]
    fn test_normalize_preserves_non_dup_parens() {
        // 非纯数字括号保留（版本号、平台标识等）
        assert_eq!(normalize_filename_for_search("app (x64).zip"), "app (x64).zip");
        assert_eq!(normalize_filename_for_search("tool (beta).exe"), "tool (beta).exe");
        // 4 位数字（年份）不算重复后缀
        assert_eq!(normalize_filename_for_search("report (2024).pdf"), "report (2024).pdf");
    }

    #[test]
    fn test_should_search_skips_hashes() {
        // 纯十六进制 ≥16 字符 → 跳过
        assert!(!should_search("3c805586e5844fa8a4cc730e843662a7.zip"));
        // 纯数字 → 跳过
        assert!(!should_search("12345.zip"));
        // 正常名 → 搜
        assert!(should_search("BeeCount-main.zip"));
        assert!(should_search("PixPin_2.0.0.3.exe"));
        assert!(should_search("中文工具.zip"));
    }

    #[test]
    fn test_score_candidate_perfect() {
        // query == repo name → 完美
        let c = RepoCandidate {
            full_name: "user/croc".into(),
            name: "croc".into(),
            description: "".into(),
            stars: 0,
            html_url: "".into(),
            topics: vec![],
            homepage: "".into(),
        };
        let score = score_candidate("croc", &c);
        assert!(score >= 1.0, "完美匹配应 ≥1.0, 实际 {}", score);
    }

    #[test]
    fn test_score_candidate_strong_prefix() {
        // repo name 以 query 开头 → 强
        let c = RepoCandidate {
            full_name: "u/easytier".into(),
            name: "easytier".into(),
            description: "".into(),
            stars: 100,
            html_url: "".into(),
            topics: vec![],
            homepage: "".into(),
        };
        // query "easytier-windows-x86_64-v2.4.5" 以 "easytier" 开头？不，反过来。
        // 实际：query_lower.starts_with(name_lower) → easytier-windows... 以 easytier 开头 → 0.7
        let score = score_candidate("easytier-windows-x86_64-v2.4.5", &c);
        assert!(score >= 0.7, "前缀匹配应 ≥0.7, 实际 {}", score);
    }

    #[test]
    fn test_score_candidate_weak_filtered() {
        // PixPin → pixpin-manager（弱，子串但 manager 多出来）→ 子串匹配 0.4，可能过 0.5？
        // 设计：子串 0.4 + stars_bonus。低 stars 时 < 0.5 被过滤。
        let c = RepoCandidate {
            full_name: "u/pixpin-manager".into(),
            name: "pixpin-manager".into(),
            description: "".into(),
            stars: 8, // 低 star
            html_url: "".into(),
            topics: vec![],
            homepage: "".into(),
        };
        let score = score_candidate("pixpin", &c);
        // pixpin 是 pixpin-manager 的子串 → 0.4 + log10(9)/10 ≈ 0.095 → ~0.495 < 0.5 被过滤
        assert!(score < 0.5, "弱匹配应 <0.5 被过滤, 实际 {}", score);
    }

    #[test]
    fn test_score_candidate_no_match() {
        let c = RepoCandidate {
            full_name: "u/totally-unrelated".into(),
            name: "totally-unrelated".into(),
            description: "".into(),
            stars: 10000,
            html_url: "".into(),
            topics: vec![],
            homepage: "".into(),
        };
        let score = score_candidate("pixpin", &c);
        assert!(score < 0.1, "无关联应 ~0, 实际 {}", score);
    }
}
