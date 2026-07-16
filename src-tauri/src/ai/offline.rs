use std::future::Future;
use std::pin::Pin;

use crate::ai::enricher::{default_enrich_result, EnrichRequest, EnrichResult, Enricher};

// ────────────────── 离线数据条目 ──────────────────

#[derive(Debug, Clone, serde::Deserialize)]
pub struct OfflineEntry {
    pub name: String,
    pub description: String,
    pub homepage_url: String,
    pub download_url: String,
    pub latest_version: String,
    pub license: String,
    pub functional_category: String,
    pub tags: Vec<String>,
}

// ────────────────── OfflineEnricher ──────────────────

pub struct OfflineEnricher {
    db: Option<std::sync::Mutex<rusqlite::Connection>>,
}

impl OfflineEnricher {
    /// 从指定路径打开离线知识库 SQLite 数据库。
    ///
    /// 如果 DB 文件不存在、打开失败或 knowledge 表为空，
    /// 则用内置的 25+ 个预置条目（default_offline_entries）建立内存库，
    /// 确保常见工具（nmap/python/yakit 等）即使无外部 db 文件也能匹配。
    pub fn new(db_path: &str) -> Self {
        // 尝试打开外部 db 文件
        let external_db = rusqlite::Connection::open(db_path).ok().and_then(|conn| {
            // 检查 knowledge 表是否有数据
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM knowledge", [], |r| r.get(0))
                .unwrap_or(0);
            if count > 0 {
                Some(conn)
            } else {
                None
            }
        });

        let db = match external_db {
            Some(conn) => Some(std::sync::Mutex::new(conn)),
            None => {
                // 外部 db 不可用 → 用预置条目建内存库
                log::info!(
                    "OfflineEnricher: 外部知识库不可用或为空，使用内置预置条目（{} 条）",
                    default_offline_entries().len()
                );
                match create_in_memory_db(&default_offline_entries()) {
                    Ok(conn) => Some(std::sync::Mutex::new(conn)),
                    Err(e) => {
                        log::warn!("OfflineEnricher: 建立内存知识库失败: {}", e);
                        None
                    }
                }
            }
        };
        Self { db }
    }

    /// 对文件名做归一化处理：小写、去扩展名、去分隔符、去平台后缀、截取前12字符。
    pub fn normalize_for_match(name: &str) -> String {
        let s = name.to_lowercase();
        // 去扩展名（取第一个 . 之前的部分）
        let s = match s.split('.').next() {
            Some(part) => part,
            None => &s,
        };
        // 去常见分隔符和括号
        let s = s.replace(['-', '_', ' ', '(', ')', '[', ']'], "");
        // 去平台/安装相关后缀
        let s = s.replace("win64", "")
            .replace("win32", "")
            .replace("x64", "")
            .replace("x86", "")
            .replace("amd64", "")
            .replace("setup", "")
            .replace("install", "")
            .replace("portable", "")
            .replace("exe", "")
            .replace("msi", "")
            .replace("zip", "");
        // 截取前 12 字符
        s.chars().take(12).collect()
    }

    /// 在知识库中查询匹配的条目。
    fn query_db(&self, req_name: &str) -> Option<EnrichResult> {
        let db = self.db.as_ref()?;
        let conn = db.lock().ok()?;
        let normalized = Self::normalize_for_match(req_name);

        let mut stmt = conn
            .prepare(
                "SELECT description, homepage_url, download_url, latest_version, \
                 license, functional_category, tags \
                 FROM knowledge WHERE normalized_name = ?1",
            )
            .ok()?;

        let row = stmt.query_row(rusqlite::params![normalized], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        });

        match row {
            Ok((description, homepage_url, download_url, latest_version, license, functional_category, tags_json)) => {
                let tags: Vec<String> = serde_json::from_str(&tags_json).unwrap_or_default();
                Some(EnrichResult {
                    description,
                    homepage_url,
                    download_url,
                    latest_version,
                    license,
                    // 离线库的 functional_category 是粗类（network/dev/Security 等），
                    // 与 DB func_categories 表的细分命名空间（Exp-Frameworks/Editor 等）不一致，
                    // 直接落库会污染命名空间且 normalize_functional_category 无法归一化。
                    // 这里置空，改由 classify_functional（扫描时）或 LLM（enrich 时）填充细分分类。
                    functional_category: String::new(),
                    tags,
                    confidence: 0.85,
                    needs_review: false,
                    provider: "offline".to_string(),
                    // 离线库条目均为官方知名源，可靠性视为 high
                    download_reliability: "high".to_string(),
                })
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => None,
            Err(e) => {
                log::warn!("OfflineEnricher query error: {}", e);
                None
            }
        }
    }
}

impl Enricher for OfflineEnricher {
    fn enrich(
        &self,
        req: EnrichRequest,
        _categories: Vec<String>,
    ) -> Pin<Box<dyn Future<Output = Result<EnrichResult, Box<dyn std::error::Error + Send + Sync>>> + Send + '_>>
    {
        // 同步查询 SQLite，将结果移入 async block
        let result = self.query_db(&req.name);

        Box::pin(async move {
            match result {
                Some(r) => Ok(r),
                None => Ok(default_enrich_result("offline")),
            }
        })
    }

    fn name(&self) -> &str {
        "offline"
    }
}

// ────────────────── 创建离线数据库 ──────────────────

/// 创建离线知识库 SQLite 数据库并写入条目。
pub fn create_offline_db(
    db_path: &str,
    entries: &[OfflineEntry],
) -> Result<(), Box<dyn std::error::Error>> {
    let db = rusqlite::Connection::open(db_path)?;

    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS knowledge (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            description TEXT DEFAULT '',
            homepage_url TEXT DEFAULT '',
            download_url TEXT DEFAULT '',
            latest_version TEXT DEFAULT '',
            license TEXT DEFAULT '',
            functional_category TEXT DEFAULT '',
            tags TEXT DEFAULT '[]'
        );
        CREATE UNIQUE INDEX IF NOT EXISTS idx_normalized_name ON knowledge(normalized_name);",
    )?;

    let mut stmt = db.prepare(
        "INSERT OR REPLACE INTO knowledge \
         (name, normalized_name, description, homepage_url, download_url, \
          latest_version, license, functional_category, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;

    for entry in entries {
        let normalized = OfflineEnricher::normalize_for_match(&entry.name);
        let tags_json = serde_json::to_string(&entry.tags)?;
        stmt.execute(rusqlite::params![
            entry.name,
            normalized,
            entry.description,
            entry.homepage_url,
            entry.download_url,
            entry.latest_version,
            entry.license,
            entry.functional_category,
            tags_json,
        ])?;
    }

    Ok(())
}

/// 用预置条目建立内存 SQLite 知识库（不依赖外部 db 文件）。
fn create_in_memory_db(entries: &[OfflineEntry]) -> Result<rusqlite::Connection, Box<dyn std::error::Error>> {
    let db = rusqlite::Connection::open_in_memory()?;
    db.execute_batch(
        "CREATE TABLE knowledge (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            normalized_name TEXT NOT NULL,
            description TEXT DEFAULT '',
            homepage_url TEXT DEFAULT '',
            download_url TEXT DEFAULT '',
            latest_version TEXT DEFAULT '',
            license TEXT DEFAULT '',
            functional_category TEXT DEFAULT '',
            tags TEXT DEFAULT '[]'
        );
        CREATE UNIQUE INDEX idx_normalized_name ON knowledge(normalized_name);",
    )?;
    let mut stmt = db.prepare(
        "INSERT OR REPLACE INTO knowledge \
         (name, normalized_name, description, homepage_url, download_url, \
          latest_version, license, functional_category, tags) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for entry in entries {
        let normalized = OfflineEnricher::normalize_for_match(&entry.name);
        let tags_json = serde_json::to_string(&entry.tags)?;
        stmt.execute(rusqlite::params![
            entry.name,
            normalized,
            entry.description,
            entry.homepage_url,
            entry.download_url,
            entry.latest_version,
            entry.license,
            entry.functional_category,
            tags_json,
        ])?;
    }
    drop(stmt);
    Ok(db)
}

// ────────────────── 预置条目 ──────────────────

/// 返回 25 个预置的常用软件离线条目。
pub fn default_offline_entries() -> Vec<OfflineEntry> {
    vec![
        OfflineEntry {
            name: "nmap".into(),
            description: "网络发现和安全审计工具，支持端口扫描和OS检测".into(),
            homepage_url: "https://nmap.org".into(),
            download_url: "https://nmap.org/download.html".into(),
            latest_version: "7.94".into(),
            license: "NPSL".into(),
            functional_category: "network".into(),
            tags: vec!["security".into(), "scanner".into()],
        },
        OfflineEntry {
            name: "python".into(),
            description: "通用编程语言，广泛用于数据科学、自动化和Web开发".into(),
            homepage_url: "https://www.python.org".into(),
            download_url: "https://www.python.org/downloads/".into(),
            latest_version: "3.12".into(),
            license: "PSF".into(),
            functional_category: "dev".into(),
            tags: vec!["language".into(), "programming".into()],
        },
        OfflineEntry {
            name: "wireshark".into(),
            description: "网络协议分析器，用于捕获和检查网络数据包".into(),
            homepage_url: "https://www.wireshark.org".into(),
            download_url: "https://www.wireshark.org/download.html".into(),
            latest_version: "4.2".into(),
            license: "GPL-2.0".into(),
            functional_category: "network".into(),
            tags: vec!["analyzer".into(), "packet".into()],
        },
        OfflineEntry {
            name: "node".into(),
            description: "基于Chrome V8引擎的JavaScript运行时环境".into(),
            homepage_url: "https://nodejs.org".into(),
            download_url: "https://nodejs.org/en/download".into(),
            latest_version: "20.11".into(),
            license: "MIT".into(),
            functional_category: "dev".into(),
            tags: vec!["javascript".into(), "runtime".into()],
        },
        OfflineEntry {
            name: "git".into(),
            description: "分布式版本控制系统，用于代码协作与版本管理".into(),
            homepage_url: "https://git-scm.com".into(),
            download_url: "https://git-scm.com/downloads".into(),
            latest_version: "2.43".into(),
            license: "GPL-2.0".into(),
            functional_category: "dev".into(),
            tags: vec!["vcs".into(), "version".into()],
        },
        OfflineEntry {
            name: "vscode".into(),
            description: "微软开发的轻量级跨平台代码编辑器".into(),
            homepage_url: "https://code.visualstudio.com".into(),
            download_url: "https://code.visualstudio.com/Download".into(),
            latest_version: "1.86".into(),
            license: "MIT".into(),
            functional_category: "dev".into(),
            tags: vec!["editor".into(), "ide".into()],
        },
        OfflineEntry {
            name: "vlc".into(),
            description: "开源跨平台多媒体播放器，支持多种音视频格式".into(),
            homepage_url: "https://www.videolan.org".into(),
            download_url: "https://www.videolan.org/vlc/".into(),
            latest_version: "3.0.20".into(),
            license: "GPL-2.1".into(),
            functional_category: "media".into(),
            tags: vec!["player".into(), "video".into()],
        },
        OfflineEntry {
            name: "7zip".into(),
            description: "高压缩比文件压缩和解压工具".into(),
            homepage_url: "https://www.7-zip.org".into(),
            download_url: "https://www.7-zip.org/download.html".into(),
            latest_version: "23.01".into(),
            license: "LGPL".into(),
            functional_category: "utility".into(),
            tags: vec!["archive".into(), "compress".into()],
        },
        OfflineEntry {
            name: "putty".into(),
            description: "SSH和Telnet远程登录客户端".into(),
            homepage_url: "https://www.putty.org".into(),
            download_url: "https://www.putty.org/download.html".into(),
            latest_version: "0.80".into(),
            license: "MIT".into(),
            functional_category: "network".into(),
            tags: vec!["ssh".into(), "remote".into()],
        },
        OfflineEntry {
            name: "curl".into(),
            description: "命令行数据传输工具，支持多种网络协议".into(),
            homepage_url: "https://curl.se".into(),
            download_url: "https://curl.se/download.html".into(),
            latest_version: "8.5".into(),
            license: "MIT".into(),
            functional_category: "network".into(),
            tags: vec!["download".into(), "http".into()],
        },
        OfflineEntry {
            name: "hutool".into(),
            description: "Java工具类库，简化日常开发任务".into(),
            homepage_url: "https://hutool.cn".into(),
            download_url: "https://hutool.cn/docs/".into(),
            latest_version: "5.8.25".into(),
            license: "Apache-2.0".into(),
            functional_category: "dev".into(),
            tags: vec!["java".into(), "library".into()],
        },
        OfflineEntry {
            name: "docker".into(),
            description: "容器化平台，用于构建、部署和管理应用容器".into(),
            homepage_url: "https://www.docker.com".into(),
            download_url: "https://www.docker.com/products/docker-desktop/".into(),
            latest_version: "24.0".into(),
            license: "Apache-2.0".into(),
            functional_category: "dev".into(),
            tags: vec!["container".into(), "virtualization".into()],
        },
        OfflineEntry {
            name: "go".into(),
            description: "Google开发的编译型编程语言，注重简洁和高效".into(),
            homepage_url: "https://go.dev".into(),
            download_url: "https://go.dev/dl/".into(),
            latest_version: "1.21".into(),
            license: "BSD-3".into(),
            functional_category: "dev".into(),
            tags: vec!["language".into(), "programming".into()],
        },
        OfflineEntry {
            name: "rust".into(),
            description: "系统级编程语言，注重安全、并发和性能".into(),
            homepage_url: "https://www.rust-lang.org".into(),
            download_url: "https://www.rust-lang.org/tools/install".into(),
            latest_version: "1.75".into(),
            license: "MIT/Apache-2.0".into(),
            functional_category: "dev".into(),
            tags: vec!["language".into(), "programming".into()],
        },
        OfflineEntry {
            name: "ffmpeg".into(),
            description: "音视频处理工具集，支持转码、剪辑和流媒体".into(),
            homepage_url: "https://ffmpeg.org".into(),
            download_url: "https://ffmpeg.org/download.html".into(),
            latest_version: "6.1".into(),
            license: "LGPL/GPL".into(),
            functional_category: "media".into(),
            tags: vec!["video".into(), "audio".into()],
        },
        OfflineEntry {
            name: "everything".into(),
            description: "Windows文件快速搜索工具，基于NTFS索引".into(),
            homepage_url: "https://www.voidtools.com".into(),
            download_url: "https://www.voidtools.com/downloads/".into(),
            latest_version: "1.4.1".into(),
            license: "MIT".into(),
            functional_category: "utility".into(),
            tags: vec!["search".into(), "file".into()],
        },
        OfflineEntry {
            name: "obs".into(),
            description: "开源直播和录屏软件，支持多源视频合成".into(),
            homepage_url: "https://obsproject.com".into(),
            download_url: "https://obsproject.com/download".into(),
            latest_version: "30.0".into(),
            license: "GPL-2.0".into(),
            functional_category: "media".into(),
            tags: vec!["streaming".into(), "recording".into()],
        },
        OfflineEntry {
            name: "notepad++".into(),
            description: "Windows平台轻量级文本和源代码编辑器".into(),
            homepage_url: "https://notepad-plus-plus.org".into(),
            download_url: "https://notepad-plus-plus.org/downloads/".into(),
            latest_version: "8.6.2".into(),
            license: "GPL-3.0".into(),
            functional_category: "dev".into(),
            tags: vec!["editor".into(), "text".into()],
        },
        OfflineEntry {
            name: "chrome".into(),
            description: "Google开发的Chromium内核网页浏览器".into(),
            homepage_url: "https://www.google.com/chrome/".into(),
            download_url: "https://www.google.com/chrome/".into(),
            latest_version: "121".into(),
            license: "proprietary".into(),
            functional_category: "browser".into(),
            tags: vec!["browser".into(), "web".into()],
        },
        OfflineEntry {
            name: "firefox".into(),
            description: "Mozilla基金会开发的开源网页浏览器".into(),
            homepage_url: "https://www.mozilla.org/firefox/".into(),
            download_url: "https://www.mozilla.org/firefox/download/".into(),
            latest_version: "122".into(),
            license: "MPL-2.0".into(),
            functional_category: "browser".into(),
            tags: vec!["browser".into(), "web".into()],
        },
        OfflineEntry {
            name: "filezilla".into(),
            description: "跨平台FTP/SFTP客户端，用于文件传输".into(),
            homepage_url: "https://filezilla-project.org".into(),
            download_url: "https://filezilla-project.org/download.php".into(),
            latest_version: "3.66".into(),
            license: "GPL-2.0".into(),
            functional_category: "network".into(),
            tags: vec!["ftp".into(), "transfer".into()],
        },
        OfflineEntry {
            name: "vmware".into(),
            description: "桌面虚拟化解决方案，可在PC上运行多个操作系统".into(),
            homepage_url: "https://www.vmware.com".into(),
            download_url: "https://www.vmware.com/products/workstation-pro.html".into(),
            latest_version: "17.5".into(),
            license: "proprietary".into(),
            functional_category: "utility".into(),
            tags: vec!["virtualization".into(), "vm".into()],
        },
        OfflineEntry {
            name: "postman".into(),
            description: "API开发和测试协作平台".into(),
            homepage_url: "https://www.postman.com".into(),
            download_url: "https://www.postman.com/downloads/".into(),
            latest_version: "10.23".into(),
            license: "proprietary".into(),
            functional_category: "dev".into(),
            tags: vec!["api".into(), "testing".into()],
        },
        OfflineEntry {
            name: "navicat".into(),
            description: "数据库可视化管理与开发工具".into(),
            homepage_url: "https://www.navicat.com".into(),
            download_url: "https://www.navicat.com/download".into(),
            latest_version: "16.3".into(),
            license: "proprietary".into(),
            functional_category: "dev".into(),
            tags: vec!["database".into(), "mysql".into()],
        },
        OfflineEntry {
            name: "jdk".into(),
            description: "Java标准版开发工具包".into(),
            homepage_url: "https://www.oracle.com/java/technologies/downloads/".into(),
            download_url: "https://www.oracle.com/java/technologies/downloads/".into(),
            latest_version: "21".into(),
            license: "GPL-2.0-with-classpath-exception".into(),
            functional_category: "dev".into(),
            tags: vec!["java".into(), "runtime".into()],
        },
        // ── 安全工具（P1-1 补充：覆盖安全场景高频工具）──
        OfflineEntry {
            name: "yakit".into(),
            description: "网络安全实战工具平台，基于 Yak 语言".into(),
            homepage_url: "https://yaklang.com".into(),
            download_url: "https://yaklang.com/products/latest".into(),
            latest_version: "".into(),
            license: "Apache-2.0".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "pentest".into()],
        },
        OfflineEntry {
            name: "yaklang".into(),
            description: "网络安全脚本语言与工具链".into(),
            homepage_url: "https://yaklang.com".into(),
            download_url: "https://github.com/yaklang/yaklang".into(),
            latest_version: "".into(),
            license: "Apache-2.0".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "language".into()],
        },
        OfflineEntry {
            name: "burpsuite".into(),
            description: "Web应用安全测试与渗透测试平台".into(),
            homepage_url: "https://portswigger.net/burp".into(),
            download_url: "https://portswigger.net/burp/releases".into(),
            latest_version: "".into(),
            license: "proprietary".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "web".into(), "proxy".into()],
        },
        OfflineEntry {
            name: "ida".into(),
            description: "交互式反汇编器，用于二进制逆向分析".into(),
            homepage_url: "https://hex-rays.com/ida-pro".into(),
            download_url: "https://hex-rays.com/ida-pro".into(),
            latest_version: "".into(),
            license: "proprietary".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "reverse".into()],
        },
        OfflineEntry {
            name: "frp".into(),
            description: "快速反向代理工具，用于内网穿透".into(),
            homepage_url: "https://github.com/fatedier/frp".into(),
            download_url: "https://github.com/fatedier/frp/releases".into(),
            latest_version: "".into(),
            license: "Apache-2.0".into(),
            functional_category: "Security".into(),
            tags: vec!["proxy".into(), "tunnel".into()],
        },
        OfflineEntry {
            name: "cobaltstrike".into(),
            description: "高级威胁模拟与后渗透测试平台".into(),
            homepage_url: "https://www.cobaltstrike.com".into(),
            download_url: "https://www.cobaltstrike.com".into(),
            latest_version: "".into(),
            license: "proprietary".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "c2".into(), "pentest".into()],
        },
        OfflineEntry {
            name: "sqlmap".into(),
            description: "自动化SQL注入检测与利用工具".into(),
            homepage_url: "https://sqlmap.org".into(),
            download_url: "https://github.com/sqlmapproject/sqlmap".into(),
            latest_version: "".into(),
            license: "GPL-2.0".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "sqli".into()],
        },
        OfflineEntry {
            name: "metasploit".into(),
            description: "渗透测试框架，含漏洞利用模块".into(),
            homepage_url: "https://www.metasploit.com".into(),
            download_url: "https://www.metasploit.com/download".into(),
            latest_version: "".into(),
            license: "BSD-3-Clause".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "exploit".into()],
        },
        OfflineEntry {
            name: "hashcat".into(),
            description: "高性能密码恢复工具，支持GPU加速".into(),
            homepage_url: "https://hashcat.net".into(),
            download_url: "https://hashcat.net/hashcat".into(),
            latest_version: "".into(),
            license: "MIT".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "crack".into()],
        },
        OfflineEntry {
            name: "john".into(),
            description: "密码破解工具（John the Ripper）".into(),
            homepage_url: "https://www.openwall.com/john".into(),
            download_url: "https://www.openwall.com/john".into(),
            latest_version: "".into(),
            license: "GPL-2.0".into(),
            functional_category: "Security".into(),
            tags: vec!["security".into(), "crack".into()],
        },
    ]
}
