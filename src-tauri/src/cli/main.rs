//! FileSweep CLI — 文件重复/旧版检测与清理工具
//!
//! 命令行入口点，独立于 Tauri GUI。
//! 用法：
//!   filesweep-cli scan --dir <路径> [--recursive] [--output <db>]
//!   filesweep-cli clean --dir <路径> [--confirm] [--dry-run]
//!   filesweep-cli enrich [--ai-provider <name>] [--skip-private] [--concurrency <n>]
//!   filesweep-cli export [--format csv|markdown] [--output <path>]
//!   filesweep-cli config [--set key=value]
//!   filesweep-cli serve [--port <n>] [--host <addr>]
//!   filesweep-cli update-db [--from-file <json>]

use clap::{Parser, Subcommand};
use std::path::PathBuf;

use filesweep_lib::core::classifier::Classifier;
use filesweep_lib::core::config::Config;
use filesweep_lib::core::dedup::DedupDetector;
use filesweep_lib::core::executor::{Executor, ExecutorAction, Operation};
use filesweep_lib::core::models::{CatalogEntry, FileRecord};
use filesweep_lib::core::privacy::PrivacyChecker;
use filesweep_lib::db::catalog::CatalogDB;

// ────────────────── CLI 定义 ──────────────────

#[derive(Parser)]
#[command(name = "filesweep", version, about = "FileSweep - 智能文件整理与软件知识库工具")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// 配置文件路径
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// 详细输出
    #[arg(short, long, global = true)]
    verbose: bool,

    /// 预览模式，不执行实际更改
    #[arg(long, global = true)]
    dry_run: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// 扫描目录中的文件，计算哈希并分类
    Scan {
        /// 扫描目录路径
        #[arg(long)]
        dir: String,

        /// 递归扫描子目录
        #[arg(long)]
        recursive: bool,

        /// 输出数据库路径
        #[arg(long)]
        output: Option<String>,
    },

    /// 清理重复和旧版文件
    Clean {
        /// 清理目录路径
        #[arg(long)]
        dir: String,

        /// 确认执行清理（否则仅预览）
        #[arg(long)]
        confirm: bool,
    },

    /// AI 丰富文件元数据
    Enrich {
        /// AI 提供者 (openai/claude/ollama/offline)
        #[arg(long)]
        ai_provider: Option<String>,

        /// 跳过私有/敏感文件
        #[arg(long)]
        skip_private: bool,

        /// 并发请求数
        #[arg(long, default_value = "5")]
        concurrency: i32,
    },

    /// 导出数据为 CSV 或 Markdown
    Export {
        /// 导出格式 (csv/markdown)
        #[arg(long, default_value = "csv")]
        format: String,

        /// 输出文件路径
        #[arg(long)]
        output: Option<String>,
    },

    /// 管理配置
    Config {
        /// 设置配置项 (格式: key=value)
        #[arg(long)]
        set: Option<String>,
    },

    /// 启动 GUI 界面（本 CLI 仅提供提示）
    Serve {
        /// WebUI 服务端口
        #[arg(long, default_value_t = 8080)]
        port: u16,

        /// WebUI 服务地址
        #[arg(long, default_value = "0.0.0.0")]
        host: String,
    },

    /// 更新离线知识库
    UpdateDb {
        /// 从 JSON 文件加载自定义知识库条目
        #[arg(long)]
        from_file: Option<String>,
    },
}

// ────────────────── 主入口 ──────────────────

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // 初始化日志
    let log_level = if cli.verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    env_logger::Builder::from_default_env()
        .filter_level(log_level)
        .format_timestamp(None)
        .init();

    // 加载配置
    let config = load_config(cli.config.as_deref());

    let result: Result<(), Box<dyn std::error::Error>> = match cli.command {
        Commands::Scan {
            dir,
            recursive,
            output,
        } => cmd_scan(&config, &dir, recursive, output).await,

        Commands::Clean { dir, confirm } => cmd_clean(&config, &dir, confirm, cli.dry_run).await,

        Commands::Enrich {
            ai_provider,
            skip_private,
            concurrency,
        } => cmd_enrich(&config, ai_provider, skip_private, concurrency).await,

        Commands::Export { format, output } => cmd_export(&config, &format, output).await,

        Commands::Config { set } => cmd_config(&config, set).await,

        Commands::Serve { port, host } => cmd_serve(port, &host).await,

        Commands::UpdateDb { from_file } => cmd_update_db(&config, from_file).await,
    };

    if let Err(e) = result {
        eprintln!("错误: {}", e);
        std::process::exit(1);
    }
}

// ────────────────── 配置加载 ──────────────────

fn load_config(path: Option<&std::path::Path>) -> Config {
    match path {
        Some(p) => filesweep_lib::core::config::load_config(&p.to_string_lossy())
            .unwrap_or_else(|e| {
                eprintln!("加载配置文件失败: {}，使用默认配置", e);
                filesweep_lib::core::config::default_config()
            }),
        None => {
            let default_path = filesweep_lib::core::config::default_config_path()
                .to_string_lossy()
                .to_string();
            filesweep_lib::core::config::load_config(&default_path).unwrap_or_else(|e| {
                if cli_verbose_check() {
                    eprintln!("使用默认配置（加载 {} 失败: {}）", default_path, e);
                }
                filesweep_lib::core::config::default_config()
            })
        }
    }
}

/// 检查是否为 verbose 模式（辅助函数，简化 env_logger 初始化后的检查）
fn cli_verbose_check() -> bool {
    std::env::var("RUST_LOG")
        .map(|v| v.contains("debug") || v.contains("trace"))
        .unwrap_or(false)
}

// ────────────────── 子命令实现 ──────────────────

async fn cmd_scan(
    config: &Config,
    dir: &str,
    recursive: bool,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("开始扫描: {} (递归: {})", dir, recursive);

    let scanner = filesweep_lib::core::scanner::Scanner::new();
    let records = scanner.scan(dir, recursive, false, &[], &[], None).await?;

    println!("扫描完成: {} 个文件", records.len());

    // 加载分类规则
    let classifier = match Classifier::new(&config.rules_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("加载分类规则失败: {}，使用默认规则", e);
            Classifier::with_defaults()
        }
    };

    // 分类
    let mut classified_records = records;
    for record in &mut classified_records {
        let result = classifier.classify(record);
        record.category = result.category;
    }

    // 写入数据库
    let db_path = output.unwrap_or_else(|| config.db_path.clone());
    let db = CatalogDB::open(&db_path)?;
    db.batch_insert_file_records(&classified_records, "")?;
    println!("已保存到 {}", db_path);

    // 去重检测
    let detector = DedupDetector::new(true, 2);
    let groups = detector.detect(&classified_records);
    if !groups.is_empty() {
        let dup_count: usize = groups.iter().map(|g| g.duplicates.len()).sum();
        println!(
            "发现 {} 组重复（共 {} 个重复文件），使用 'filesweep clean' 进行清理",
            groups.len(),
            dup_count
        );
    }

    Ok(())
}

async fn cmd_clean(
    config: &Config,
    dir: &str,
    confirm: bool,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let is_dry_run = dry_run || !confirm;

    let db = CatalogDB::open(&config.db_path)?;
    let (records, _) = db.get_file_records("", "active", "", 1, 100_000)?;

    if records.is_empty() {
        println!("没有找到文件记录，请先运行 scan 命令");
        return Ok(());
    }

    let detector = DedupDetector::new(true, 2);
    let groups = detector.detect(&records);

    if groups.is_empty() {
        println!("没有发现重复文件");
        return Ok(());
    }

    let _classifier = Classifier::new(&config.rules_path).unwrap_or_else(|_| Classifier::with_defaults());

    let mut actions: Vec<ExecutorAction> = Vec::new();
    for group in &groups {
        for dup in &group.duplicates {
            let action = ExecutorAction {
                operation: Operation::Delete,
                source: dup.local_path.clone(),
                dest: String::new(),
                reason: group.reason.clone(),
                file: dup.clone(),
            };
            actions.push(action);
        }
    }

    let session_id = uuid::Uuid::new_v4().to_string()[..8].to_string();
    let use_recycle_bin = config.rules.move_to_recycle_bin;
    let mut executor = Executor::new(is_dry_run, dir.to_string());
    executor.use_recycle_bin = use_recycle_bin;

    println!("开始清理: {} 个操作 (dry-run: {})", actions.len(), is_dry_run);
    let logs = executor.execute(&actions, &session_id)?.logs;

    for log in &logs {
        db.insert_operation_log(log)?;
    }

    let mut moved = 0usize;
    let mut deleted = 0usize;
    let mut failed = 0usize;
    for log in &logs {
        match log.status.as_str() {
            "success" | "dry_run" => {
                if log.operation == "MOVE" {
                    moved += 1;
                } else {
                    deleted += 1;
                }
            }
            "error" => failed += 1,
            _ => {}
        }
    }

    if is_dry_run {
        println!("[预览模式] 将移动 {} 个文件，删除 {} 个文件", moved, deleted);
    } else {
        println!(
            "清理完成: 移动 {} 个，删除 {} 个，失败 {} 个",
            moved, deleted, failed
        );
    }

    Ok(())
}

async fn cmd_enrich(
    config: &Config,
    ai_provider: Option<String>,
    skip_private: bool,
    concurrency: i32,
) -> Result<(), Box<dyn std::error::Error>> {
    let provider = ai_provider.unwrap_or_else(|| config.ai.provider.clone());

    let db = CatalogDB::open(&config.db_path)?;
    let (records, _) = db.get_file_records("", "active", "", 1, 1_000_000)?;

    if records.is_empty() {
        println!("没有找到文件记录，请先运行 scan 命令");
        return Ok(());
    }

    // 过滤私有文件
    let privacy = PrivacyChecker::new(config.privacy_rules.clone());
    let mut requests = Vec::new();
    let mut valid_records: Vec<FileRecord> = Vec::new();

    for r in &records {
        if skip_private && (r.ai_skip || privacy.should_skip(r)) {
            println!("跳过私密文件: {}", r.name);
            continue;
        }
        requests.push(filesweep_lib::ai::enricher::EnrichRequest {
            name: r.name.clone(),
            version: r.version.clone(),
            extension: r.extension.clone(),
            category: r.category.clone(),
            file_size: r.file_size,
            available_tags: None,
        });
        valid_records.push(r.clone());
    }

    if requests.is_empty() {
        println!("没有需要补全的文件");
        return Ok(());
    }

    println!(
        "开始 AI 补全: {} 个文件, 提供方: {}, 并发: {}",
        requests.len(),
        provider,
        concurrency
    );

    // 获取已有标签
    let allowed_tags = db.get_all_tag_names().unwrap_or_default();
    for req in &mut requests {
        req.available_tags = Some(allowed_tags.clone());
    }

    // 获取功能分类名
    let cat_path = config.categories_path();
    let allowed_cats = load_category_names(&cat_path);

    // 创建 enricher
    let enricher = create_cli_enricher(&provider, config)?;

    let (tx, mut rx) = tokio::sync::mpsc::channel::<filesweep_lib::ai::enricher::EnrichProgress>(16);
    let verbose = std::env::var("RUST_LOG").map(|v| v.contains("debug")).unwrap_or(false);

    // 进度监听
    tokio::spawn(async move {
        while let Some(p) = rx.recv().await {
            if verbose {
                println!("补全进度: {}/{} ({})", p.done, p.total, p.current_name);
            }
        }
    });

    let results = filesweep_lib::ai::enricher::batch_enrich(
        enricher.as_ref(),
        requests,
        allowed_cats.clone(),
        concurrency.max(1) as usize,
        tx,
    )
    .await;

    let mut review_count = 0;
    for (i, result) in results.iter().enumerate() {
        if i >= valid_records.len() {
            break;
        }

        let normalized_tags = filesweep_lib::db::catalog::normalize_tags(&result.tags, &allowed_tags);
        let normalized_cat =
            filesweep_lib::db::catalog::normalize_functional_category(&result.functional_category, &allowed_cats);

        let entry = CatalogEntry {
            id: format!("cat_{}", &valid_records[i].file_hash[..8.min(valid_records[i].file_hash.len())]),
            name: valid_records[i].name.clone(),
            description: result.description.clone(),
            homepage_url: result.homepage_url.clone(),
            download_url: result.download_url.clone(),
            latest_version: result.latest_version.clone(),
            license: result.license.clone(),
            functional_category: normalized_cat.clone(),
            tags: normalized_tags,
            ai_confidence: result.confidence,
            ai_provider: result.provider.clone(),
            meta_updated_at: chrono::Utc::now(),
            notes: String::new(),
            needs_review: result.needs_review,
            ai_skip: false,
            download_reliability: result.download_reliability.clone(),
        };

        db.insert_catalog_entry(&entry)?;

        if result.needs_review {
            review_count += 1;
        }
    }

    println!(
        "补全完成: {}/{} 个文件已处理, {} 个需人工审核",
        results.len(),
        valid_records.len(),
        review_count
    );

    Ok(())
}

async fn cmd_export(
    config: &Config,
    format: &str,
    output: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let db = CatalogDB::open(&config.db_path)?;
    let (entries, _total) = db.get_catalog_entries("", 1, 1_000_000)?;

    let output_path = output.unwrap_or_else(|| {
        let dir = std::path::Path::new(&config.db_path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());
        match format {
            "markdown" | "md" => format!("{}/catalog.md", dir),
            _ => format!("{}/catalog.csv", dir),
        }
    });

    let content = match format.to_lowercase().as_str() {
        "csv" => export_csv(&entries),
        "markdown" | "md" | "obsidian" => export_obsidian_md(&entries),
        _ => return Err(format!("不支持的导出格式: {}", format).into()),
    };

    std::fs::write(&output_path, &content)?;
    println!("已导出 {} 条记录到 {}", entries.len(), output_path);

    Ok(())
}

async fn cmd_config(
    config: &Config,
    set: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    if set.is_none() {
        println!("当前配置:");
        println!("  AI Provider:    {}", config.ai.provider);
        println!("  AI Base URL:    {}", config.ai.base_url);
        println!("  AI Concurrency: {}", config.ai_concurrency);
        println!("  DB Path:        {}", config.db_path);
        println!("  Rules Path:     {}", config.rules_path);
        println!("  Port:           {}", config.port);
        println!("  Host:           {}", config.host);
        println!("  Log Level:      {}", config.log_level);
        return Ok(());
    }

    let kv = set.unwrap();
    let (key, value) = kv
        .split_once('=')
        .ok_or_else(|| "格式错误，请使用 key=value，例如: ai.provider=claude")?;

    let mut cfg = config.clone();
    match key {
        "ai.provider" | "aiProvider" => cfg.ai.provider = value.to_string(),
        "ai.apiKey" | "aiApiKey" => cfg.ai.api_key = value.to_string(),
        "ai.baseUrl" | "aiBaseUrl" => cfg.ai.base_url = value.to_string(),
        "ai.concurrency" | "aiConcurrency" => {
            cfg.ai_concurrency = value.parse().unwrap_or(cfg.ai_concurrency)
        }
        "dbPath" => cfg.db_path = value.to_string(),
        "rulesPath" => cfg.rules_path = value.to_string(),
        "port" => cfg.port = value.parse().unwrap_or(cfg.port),
        "host" => cfg.host = value.to_string(),
        "logLevel" => cfg.log_level = value.to_string(),
        _ => return Err(format!("未知的配置项: {}", key).into()),
    }

    let config_path = filesweep_lib::core::config::default_config_path()
        .to_string_lossy()
        .to_string();
    cfg.save(&config_path)?;
    println!("已设置 {} = {}", key, value);

    Ok(())
}

async fn cmd_serve(port: u16, host: &str) -> Result<(), Box<dyn std::error::Error>> {
    println!("FileSweep WebUI 启动中... http://{}:{}", host, port);
    println!("提示: 请直接运行 FileSweep 桌面应用 (Tauri GUI) 以获得完整 WebUI 体验。");
    println!("      或使用 `cargo tauri dev` 启动开发模式。");
    Ok(())
}

async fn cmd_update_db(
    config: &Config,
    from_file: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let db_path = config.offline_db_path();

    let mut entries = filesweep_lib::ai::offline::default_offline_entries();

    if let Some(file_path) = from_file {
        let data = std::fs::read_to_string(&file_path)?;
        let custom: Vec<filesweep_lib::ai::offline::OfflineEntry> = serde_json::from_str(&data)?;
        println!("从 {} 加载了 {} 条自定义条目", file_path, custom.len());
        entries.extend(custom);
    }

    filesweep_lib::ai::offline::create_offline_db(&db_path, &entries)?;
    println!("离线知识库已更新: {} 条记录 -> {}", entries.len(), db_path);

    Ok(())
}

// ────────────────── CLI 辅助函数 ──────────────────

fn create_cli_enricher(
    provider: &str,
    config: &Config,
) -> Result<Box<dyn filesweep_lib::ai::enricher::Enricher + Send + Sync>, Box<dyn std::error::Error>>
{
    match provider {
        "openai" => Ok(Box::new(filesweep_lib::ai::openai::OpenAIEnricher::new(
            &config.ai.api_key,
            &config.ai.base_url,
        ))),
        "claude" => Ok(Box::new(filesweep_lib::ai::claude::ClaudeEnricher::new(
            &config.ai.api_key,
            &config.ai.base_url,
        ))),
        "ollama" => Ok(Box::new(filesweep_lib::ai::ollama::OllamaEnricher::new(
            &config.ai.ollama_url,
        ))),
        "offline" => {
            let db_path = config.offline_db_path();
            Ok(Box::new(filesweep_lib::ai::offline::OfflineEnricher::new(
                &db_path,
            )))
        }
        _ => Err(format!("不支持的 AI 提供方: {}", provider).into()),
    }
}

fn load_category_names(path: &str) -> Vec<String> {
    if let Ok(data) = std::fs::read_to_string(path) {
        if let Ok(parsed) = serde_yaml::from_str::<serde_yaml::Value>(&data) {
            if let Some(cats) = parsed.get("categories").and_then(|v| v.as_sequence()) {
                return cats
                    .iter()
                    .filter_map(|c| c.get("name").and_then(|n| n.as_str()).map(String::from))
                    .collect();
            }
        }
    }
    Vec::new()
}

fn export_csv(entries: &[CatalogEntry]) -> String {
    let mut csv = String::from(
        "ID,Name,Description,Homepage,Download,Version,License,Category,Tags,Confidence,NeedsReview\n",
    );
    for e in entries {
        let tags_str = e.tags.join(";");
        let needs_review = if e.needs_review { "true" } else { "false" };
        csv.push_str(&format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",{},{}\n",
            e.id,
            e.name,
            e.description.replace('"', "\"\""),
            e.homepage_url,
            e.download_url,
            e.latest_version,
            e.license,
            e.functional_category,
            tags_str,
            e.ai_confidence,
            needs_review,
        ));
    }
    csv
}

fn export_obsidian_md(entries: &[CatalogEntry]) -> String {
    let mut md = String::new();
    for e in entries {
        let tags_array = e
            .tags
            .iter()
            .map(|t| format!("\"{}\"", t))
            .collect::<Vec<_>>()
            .join(", ");

        md.push_str(&format!(
            "---\n\
             id: {}\n\
             name: \"{}\"\n\
             category: {}\n\
             tags: [{}]\n\
             confidence: {}\n\
             needs_review: {}\n\
             ---\n\
             ## {}\n\
             \n\
             - **描述**: {}\n\
             - **版本**: {}\n\
             - **主页**: [链接]({})\n\
             - **下载**: [链接]({})\n\
             - **许可证**: {}\n\
             - **AI 提供方**: {}\n\
             \n\n",
            e.id,
            e.name,
            e.functional_category,
            tags_array,
            e.ai_confidence,
            e.needs_review,
            e.name,
            if e.description.is_empty() {
                "暂无".to_string()
            } else {
                e.description.clone()
            },
            e.latest_version,
            e.homepage_url,
            e.download_url,
            e.license,
            e.ai_provider,
        ));
    }
    md
}
