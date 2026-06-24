//! 软件描述：预置目录名 → 软件名映射 + AI 回退（grill Q9）。
//!
//! MVP 策略：
//!   1. 命中预置映射（内置 + DB am_describe_map）→ source='preset'
//!   2. 未命中：返回目录名作为 software_name，描述留空（前端可显式调 AI）
//! AI 接入预留 `describe_with_ai`，默认返回 None（避免无 key 时报错）。

use rusqlite::Connection;

use crate::plugins::appmover::models::DirDescription;

/// 内置预置映射（常见软件目录名 → 中文名）。
/// 用户可在 am_describe_map 表覆盖/扩展。
pub const PRESET_MAP: &[(&str, &str, &str)] = &[
    // (目录名, 软件名, 描述)
    ("Tencent", "腾讯软件", "QQ/微信等腾讯产品数据"),
    ("WeChat", "微信", "微信客户端数据"),
    ("Weixin", "微信", "微信客户端数据"),
    ("QQ", "QQ", "QQ 客户端数据"),
    ("NetEase", "网易", "网易系产品数据"),
    ("Baidu", "百度", "百度系产品数据"),
    ("Alibaba", "阿里", "阿里系产品数据"),
    ("Google", "Google", "Chrome / Google 系产品数据"),
    ("Mozilla", "Mozilla", "Firefox 等产品数据"),
    ("Adobe", "Adobe", "Adobe 系产品（PS/PR/AE 等）数据"),
    ("Notion", "Notion", "Notion 客户端数据"),
    ("Slack", "Slack", "Slack 客户端数据"),
    ("Discord", "Discord", "Discord 客户端数据"),
    ("Steam", "Steam", "Steam 游戏平台数据"),
    ("Epic*", "Epic Games", "Epic 游戏平台数据"),
    ("JetBrains", "JetBrains", "IDEA/PyCharm 等 JetBrains IDE 数据"),
    ("vscode", "VS Code", "Visual Studio Code 配置与扩展"),
    ("Code", "VS Code", "Visual Studio Code 配置"),
    ("Docker", "Docker", "Docker Desktop 数据"),
    ("npm", "npm", "Node.js npm 全局包缓存"),
    ("pip", "pip", "Python pip 缓存"),
    ("Python", "Python", "Python 相关缓存与配置"),
    ("GitHub*", "GitHub", "GitHub Desktop / CLI 数据"),
    ("Obsidian", "Obsidian", "Obsidian 笔记数据"),
    ("WPS*", "WPS", "WPS Office 数据"),
    ("Kingsoft", "金山", "金山系产品数据"),
    ("360*", "360", "360 系产品数据"),
    ("Sogou*", "搜狗", "搜狗输入法等数据"),
    ("Riot*", "Riot Games", "拳头游戏数据"),
    ("NVIDIA*", "NVIDIA", "NVIDIA 驱动与工具数据"),
];

/// 查询单个目录的描述（预置 + DB 覆盖）。
pub fn describe(db: &Connection, dir_name: &str) -> DirDescription {
    // 1. DB 用户/历史覆盖
    if let Ok(row) = db.query_row(
        "SELECT software_name, description, source FROM am_describe_map WHERE dir_name = ?1",
        rusqlite::params![dir_name],
        |r| {
            Ok(DirDescription {
                dir_name: dir_name.into(),
                software_name: r.get::<_, String>(0)?,
                description: r.get::<_, String>(1)?,
                source: r.get::<_, String>(2)?,
            })
        },
    ) {
        return row;
    }
    // 2. 预置映射（通配匹配）
    for (pat, name, desc) in PRESET_MAP {
        if matches_pattern(pat, dir_name) {
            return DirDescription {
                dir_name: dir_name.into(),
                software_name: (*name).into(),
                description: (*desc).into(),
                source: "preset".into(),
            };
        }
    }
    // 3. 兜底：用目录名
    DirDescription {
        dir_name: dir_name.into(),
        software_name: dir_name.into(),
        description: String::new(),
        source: "fallback".into(),
    }
}

/// 列出所有预置 + DB 描述（用于前端展示映射表）。
pub fn list_all(db: &Connection) -> Vec<DirDescription> {
    let mut out: Vec<DirDescription> = PRESET_MAP
        .iter()
        .map(|(d, n, desc)| DirDescription {
            dir_name: (*d).into(),
            software_name: (*n).into(),
            description: (*desc).into(),
            source: "preset".into(),
        })
        .collect();
    // DB 覆盖
    if let Ok(mut stmt) = db.prepare("SELECT dir_name, software_name, description, source FROM am_describe_map") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok(DirDescription {
                dir_name: r.get::<_, String>(0)?,
                software_name: r.get::<_, String>(1)?,
                description: r.get::<_, String>(2)?,
                source: r.get::<_, String>(3)?,
            })
        }) {
            for r in rows.flatten() {
                // 用 DB 版覆盖同名预置
                if let Some(pos) = out.iter().position(|x| x.dir_name == r.dir_name) {
                    out[pos] = r;
                } else {
                    out.push(r);
                }
            }
        }
    }
    out
}

/// 用户手动写入/更新一条描述（source='user'）。
pub fn upsert(db: &Connection, dir_name: &str, software_name: &str, description: &str) -> rusqlite::Result<()> {
    db.execute(
        "INSERT INTO am_describe_map (dir_name, software_name, description, source)
         VALUES (?1, ?2, ?3, 'user')
         ON CONFLICT(dir_name) DO UPDATE SET
            software_name = excluded.software_name,
            description = excluded.description,
            source = 'user'",
        rusqlite::params![dir_name, software_name, description],
    )?;
    Ok(())
}

/// 简单通配匹配：pat 以 * 结尾表示前缀匹配，否则精确。
fn matches_pattern(pat: &str, name: &str) -> bool {
    if let Some(prefix) = pat.strip_suffix('*') {
        name.to_ascii_lowercase().starts_with(&prefix.to_ascii_lowercase())
    } else {
        name.eq_ignore_ascii_case(pat)
    }
}

/// AI 回退接口：复用项目内 OfflineEnricher 离线知识库给目录名生成描述。
///
/// 优先级：
///   1. 用目录名作 EnrichRequest.name 查离线知识库（命中置信度 0.85）
///   2. 命中且 description 非空 → 返回 DirDescription（source='ai'）
///   3. 未命中 → None（由调用方回退到预置映射/兜底）
///
/// 选择 OfflineEnricher 是因为它不需要 API key，在任意环境都能工作。
/// 未来若配置了在线 provider，可在此处切换为 FallbackEnricher 链。
pub async fn describe_with_ai(dir_name: &str) -> Option<DirDescription> {
    use crate::ai::enricher::{EnrichRequest, Enricher};
    use crate::ai::offline::OfflineEnricher;

    // 离线知识库路径：与 catalog 同目录（复用现有约定）
    // 若路径不存在，OfflineEnricher 内部 db=None，enrich 返回空结果
    let db_path = std::env::var("FILESWEEP_KB_DB").unwrap_or_else(|_| "knowledge.db".into());
    let enricher = OfflineEnricher::new(&db_path);

    let req = EnrichRequest {
        name: dir_name.to_string(),
        version: String::new(),
        extension: String::new(),
        category: String::new(),
        file_size: 0,
        available_tags: None,
    };

    match enricher.enrich(req, vec![]).await {
        Ok(result) => {
            if result.confidence >= 0.5 && !result.description.is_empty() {
                Some(DirDescription {
                    dir_name: dir_name.to_string(),
                    software_name: dir_name.to_string(),
                    description: result.description,
                    source: "ai".to_string(),
                })
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE am_describe_map (
                dir_name TEXT PRIMARY KEY,
                software_name TEXT NOT NULL,
                description TEXT DEFAULT '',
                source TEXT DEFAULT 'preset'
            );",
        )
        .unwrap();
        db
    }

    #[test]
    fn test_preset_match() {
        let db = mem_db();
        let d = describe(&db, "WeChat");
        assert_eq!(d.software_name, "微信");
        assert_eq!(d.source, "preset");
    }

    #[test]
    fn test_preset_wildcard() {
        let db = mem_db();
        let d = describe(&db, "NVIDIA Corporation");
        assert_eq!(d.software_name, "NVIDIA");
    }

    #[test]
    fn test_db_override() {
        let db = mem_db();
        upsert(&db, "WeChat", "我的微信", "自定义").unwrap();
        let d = describe(&db, "WeChat");
        assert_eq!(d.software_name, "我的微信");
        assert_eq!(d.source, "user");
    }

    #[test]
    fn test_fallback() {
        let db = mem_db();
        let d = describe(&db, "SomeRandomDir");
        assert_eq!(d.software_name, "SomeRandomDir");
        assert_eq!(d.source, "fallback");
    }
}
