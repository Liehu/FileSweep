//! filesweep 插件 action 分发
//!
//! 复用 commands/*_headless 函数（接受普通参数，已解耦 Tauri State 注入）。
//! config 相关 action 临时构造 Arc<tokio::sync::RwLock<Config>> 以匹配 headless 签名。
//!
//! P1 实现核心 db-only action；未实现的返回 NotImplemented（双轨并存：stores 仍可用旧 invoke）。

use std::sync::Arc;

use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::PluginError;
use crate::commands;
use crate::db::config::{CategoryRuleRow, DirPatternRow, FuncCategoryRow};

/// filesweep 插件 action 分发
pub async fn dispatch(action: &str, args: Value, ctx: &Context) -> Result<Value, PluginError> {
    match action {
        // ═════════ scan ═════════
        "scan:stats" => {
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || {
                commands::scan::get_file_stats_headless(&db)
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(v)
        }
        "scan:files" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] page: Option<i32>,
                #[serde(default)] page_size: Option<i32>,
                #[serde(default)] category: Option<String>,
                #[serde(default)] status: Option<String>,
                #[serde(default)] search: Option<String>,
                #[serde(default)] dir_type: Option<String>,
                #[serde(default)] task_id: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let page = a.page.unwrap_or(1);
            let page_size = a.page_size.unwrap_or(50);
            let category = a.category;
            let status = a.status;
            let search = a.search;
            let dir_type = a.dir_type;
            let task_id = a.task_id;
            // 用 spawn_blocking 避免阻塞 tokio runtime
            let result = tokio::task::spawn_blocking(move || {
                commands::scan::get_files_headless(
                    &db, page, page_size, category, status, search, dir_type, task_id,
                )
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))?;
            Ok(result?)
        }
        // 扫描任务列表（历史扫描记录）
        "scan:tasks:list" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default = "default_task_limit")] limit: i64,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || db.list_scan_tasks(a.limit))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(rows)?)
        }
        "scan:tasks:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.delete_scan_task(&a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }
        "scan:suggestions" => {
            // headless 版本需要 Arc<tokio::sync::RwLock<Config>>，从 Context 读出 Config 值包装
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::scan::get_suggestions_headless(&ctx.db, &tok_cfg).await?)
        }
        "scan:suggestions_v2" => {
            // 智能建议引擎（分组返回：高置信/需确认/旧版本/重复）
            let db = ctx.db.clone();
            let cfg = ctx.config.read().clone();
            let result = tokio::task::spawn_blocking(move || -> Result<Value, String> {
                let (records, _) = db
                    .get_file_records("", "active", "", 1, 100_000)
                    .map_err(|e| format!("查询文件失败: {}", e))?;

                // 查询 catalog 条目（AI 丰富数据）
                let catalogs = db
                    .get_catalog_entries("", 1, 100_000)
                    .map_err(|e| format!("查询 catalog 失败: {}", e))?
                    .0;

                // 去重检测
                let detector = crate::core::dedup::DedupDetector::new(true, 2);
                let groups = detector.detect(&records);

                // 查询功能分类表（用于"按功能用途整理到细分目录"建议）
                let func_categories = db.list_func_categories().unwrap_or_default();

                // 生成建议
                let summary = crate::core::suggestion::generate_suggestions(
                    &records,
                    &catalogs,
                    &groups,
                    &func_categories,
                );
                serde_json::to_value(summary).map_err(|e| format!("序列化失败: {}", e))
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))?;
            Ok(result?)
        }

        // ═════════ search（Everything 集成）═════════
        "search" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                query: String,
                #[serde(default = "default_max_results")] max_results: usize,
            }
            let a: Args = serde_json::from_value(args)?;
            // 尝试 Everything，失败则回退到 DB 搜索
            if crate::core::everything::is_everything_available() {
                let results = crate::core::everything::search_with_everything(&a.query, a.max_results)?;
                Ok(serde_json::to_value(results)?)
            } else {
                // 回退：DB 内搜索
                let db = ctx.db.clone();
                let q = a.query.clone();
                let results = tokio::task::spawn_blocking(move || -> Result<Value, String> {
                    let (files, total) = db
                        .get_file_records("", "active", &q, 1, 100)
                        .map_err(|e| format!("DB 搜索失败: {}", e))?;
                    Ok(serde_json::json!({
                        "results": files.iter().map(|f| serde_json::json!({
                            "name": f.name,
                            "path": f.local_path,
                            "size": f.file_size,
                        })).collect::<Vec<_>>(),
                        "total": total,
                        "source": "database",
                    }))
                })
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))?;
                Ok(results?)
            }
        }

        // ═════════ catalog ═════════
        "catalog:get" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] page: Option<i32>,
                #[serde(default)] page_size: Option<i32>,
                #[serde(default)] search: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::catalog::get_catalog_headless(
                &ctx.db,
                a.page.unwrap_or(1),
                a.page_size.unwrap_or(50),
                a.search,
            )?)
        }
        "catalog:export" => {
            #[derive(serde::Deserialize)]
            struct Args { format: String }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::catalog::export_catalog_headless(&ctx.db, &a.format)?)
        }
        "catalog:update" => {
            Ok(commands::catalog::update_catalog_entry_headless(&ctx.db, args)?)
        }
        "catalog:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { ids: Vec<String> }
            let a: Args = serde_json::from_value(args)?;
            // headless 版本接受单个 id，批量删除逐个调用
            let results: Vec<Value> = a.ids.iter().map(|id| {
                commands::catalog::delete_catalog_entry_headless(&ctx.db, id.clone())
                    .unwrap_or_else(|e| serde_json::json!({"error": e}))
            }).collect();
            Ok(Value::Array(results))
        }

        // ═════════ tags ═════════
        "tags:get" => Ok(commands::tags::get_tags_headless(&ctx.db)?),
        "tags:create" => {
            #[derive(serde::Deserialize)]
            struct Args { name: String, color: String, description: String }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::tags::create_tag_headless(&ctx.db, a.name, a.color, a.description)?)
        }
        "tags:update" => Ok(commands::tags::update_tag_headless(&ctx.db, args)?),
        "tags:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: String }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::tags::delete_tag_headless(&ctx.db, a.id)?)
        }

        // ═════════ logs ═════════
        "logs:get" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] page: Option<i32>,
                #[serde(default)] page_size: Option<i32>,
                #[serde(default)] action: Option<String>,
                #[serde(default)] status: Option<String>,
                #[serde(default)] q: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::logs::get_logs_headless(
                &ctx.db,
                a.page.unwrap_or(1),
                a.page_size.unwrap_or(50),
                a.action,
                a.status,
                a.q,
            )?)
        }
        "logs:revert" => {
            #[derive(serde::Deserialize)]
            struct Args { id: i64 }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::logs::revert_operation_headless(&ctx.db, a.id)?)
        }
        "logs:batch_revert" => {
            #[derive(serde::Deserialize)]
            struct Args { ids: Vec<i64> }
            let a: Args = serde_json::from_value(args)?;
            Ok(commands::logs::batch_revert_headless(&ctx.db, a.ids)?)
        }

        // ═════════ db_ops ═════════
        "db:reset" => Ok(commands::db_ops::reset_db_headless(&ctx.db)?),

        // ═════════ rules / categories / settings（需 config，临时包装 tokio RwLock）═════════
        "rules:get" => {
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::rules::get_rules_headless(&tok_cfg).await?)
        }
        "rules:update" => {
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::rules::update_rules_headless(&tok_cfg, args).await?)
        }
        "categories:get" => {
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::categories::get_func_categories_headless(&tok_cfg).await?)
        }
        "categories:update" => {
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::categories::update_func_categories_headless(&tok_cfg, args).await?)
        }
        "settings:get" => {
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::settings::get_settings_headless(&tok_cfg).await?)
        }
        "settings:update" => {
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            let result = commands::settings::update_settings_headless(&tok_cfg, args).await?;
            // 关键：把更新后的 cfg 写回全局 ctx.config（parking_lot::RwLock）。
            // 否则 update_settings_headless 只更新了临时 tok_cfg，全局 ctx.config 仍是旧值，
            // 导致 settings:get / start_enrich 读到旧配置（自定义 AI 配置"保存后回旧值"的根因）。
            let updated_cfg = tok_cfg.read().await.clone();
            *ctx.config.write() = updated_cfg;
            Ok(result)
        }
        "settings:test" => {
            // AI 配置连通性+认证测试：用 args 里的 provider/url/key/model 发极简 ping 请求，
            // 成功返回 { ok, model, latency_ms }，失败返回服务端原始报错（HTTP 状态码 + body）。
            Ok(commands::settings::test_ai_connection(args).await?)
        }

        // ═════════ config:*（DB 化配置 CRUD，spawn_blocking 避免 lock 竞争）═════════
        //
        // 4 张表 × (list/add/update/delete)，全部走 CatalogDB 的 config CRUD 方法。
        // 数据量小（几十到几百行），统一用 spawn_blocking 包裹 DB 调用。
        //
        // 前端契约见 ConfigView.vue：
        //   - software_roots / exclude_rules：update 接受局部字段（Optional）
        //   - category_rules / func_categories：update 接受完整行对象（含 id + 全字段）

        // ─── software_roots ───
        "config:roots:list" => {
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || db.list_software_roots())
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(rows)?)
        }
        "config:roots:add" => {
            #[derive(serde::Deserialize)]
            struct Args {
                path: String,
                #[serde(default)] display_name: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let row = tokio::task::spawn_blocking(move || {
                db.add_software_root(&a.path, &a.display_name)
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(row)?)
        }
        "config:roots:update" => {
            #[derive(serde::Deserialize)]
            struct Args {
                id: i64,
                #[serde(default)] path: Option<String>,
                #[serde(default)] display_name: Option<String>,
                #[serde(default)] enabled: Option<bool>,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || {
                db.update_software_root(a.id, a.path.as_deref(), a.display_name.as_deref(), a.enabled)
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }
        "config:roots:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: i64 }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.delete_software_root(a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }

        // ─── category_rules ───
        "config:categories:list" => {
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || db.list_category_rules())
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(rows)?)
        }
        "config:categories:add" => {
            let row: CategoryRuleRow = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let result = tokio::task::spawn_blocking(move || db.add_category_rule(&row))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(result)?)
        }
        "config:categories:update" => {
            // 前端总是传完整行（toggle 传 {...r, enabled}，edit 也传全字段）
            let row: CategoryRuleRow = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.update_category_rule(&row))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }
        "config:categories:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: i64 }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.delete_category_rule(a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }

        // ─── func_categories ───
        "config:func_categories:list" => {
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || db.list_func_categories())
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(rows)?)
        }
        "config:func_categories:add" => {
            let row: FuncCategoryRow = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let result = tokio::task::spawn_blocking(move || db.add_func_category(&row))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(result)?)
        }
        "config:func_categories:update" => {
            let row: FuncCategoryRow = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.update_func_category(&row))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }
        "config:func_categories:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: i64 }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.delete_func_category(a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }

        // ─── dir_patterns（目录模式分类）───
        "config:patterns:list" => {
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || db.list_dir_patterns())
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(rows)?)
        }
        "config:patterns:add" => {
            let row: DirPatternRow = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let result = tokio::task::spawn_blocking(move || db.add_dir_pattern(&row))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(result)?)
        }
        "config:patterns:update" => {
            // 前端传完整行（toggle 传 {...p, enabled}，edit 传全字段）
            let row: DirPatternRow = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.update_dir_pattern(&row))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }
        "config:patterns:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: i64 }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.delete_dir_pattern(a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }

        // ─── exclude_rules ───
        "config:exclude:list" => {
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || db.list_exclude_rules())
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(rows)?)
        }
        "config:exclude:add" => {
            #[derive(serde::Deserialize)]
            struct Args {
                rule_type: String,
                pattern: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let row = tokio::task::spawn_blocking(move || {
                db.add_exclude_rule(&a.rule_type, &a.pattern)
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(serde_json::to_value(row)?)
        }
        "config:exclude:update" => {
            #[derive(serde::Deserialize)]
            struct Args {
                id: i64,
                #[serde(default)] rule_type: Option<String>,
                #[serde(default)] pattern: Option<String>,
                #[serde(default)] enabled: Option<bool>,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || {
                db.update_exclude_rule(a.id, a.rule_type.as_deref(), a.pattern.as_deref(), a.enabled)
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }
        "config:exclude:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: i64 }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || db.delete_exclude_rule(a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(Value::Null)
        }

        // ─── tags（复用 commands::tags headless wrapper，String id）───
        "config:tags:list" => {
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || commands::tags::get_tags_headless(&db))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(v)
        }
        "config:tags:add" => {
            #[derive(serde::Deserialize)]
            struct Args {
                name: String,
                color: String,
                #[serde(default)] description: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || {
                commands::tags::create_tag_headless(&db, a.name, a.color, a.description)
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(v)
        }
        "config:tags:update" => {
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || commands::tags::update_tag_headless(&db, args))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(v)
        }
        "config:tags:delete" => {
            #[derive(serde::Deserialize)]
            struct Args { id: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || commands::tags::delete_tag_headless(&db, a.id))
                .await
                .map_err(|e| format!("spawn_blocking 失败: {}", e))??;
            Ok(v)
        }

        // ═════════ files（文件操作预设）═════════
        "files:set_action" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                file_id: String,
                action: String,
                #[serde(default)]
                move_target: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            ctx.db
                .set_file_action(&a.file_id, &a.action, a.move_target.as_deref())?;
            Ok(Value::Null)
        }
        "files:set_move_target" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                file_id: String,
                target: String,
            }
            let a: Args = serde_json::from_value(args)?;
            ctx.db.set_file_action(&a.file_id, "", Some(&a.target))?;
            Ok(Value::Null)
        }
        "files:batch_set_action" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                file_ids: Vec<String>,
                action: String,
                #[serde(default)]
                move_target: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let count =
                ctx.db
                    .batch_set_action(&a.file_ids, &a.action, a.move_target.as_deref())?;
            Ok(serde_json::json!({ "updated": count }))
        }

        // ═════════ 长任务 action（broadcast → app.emit 桥接）═════════
        "scan:start" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                dirs: Vec<String>,
                #[serde(default = "default_true")] recursive: bool,
                #[serde(default)] exclude_dirs: Vec<String>,
                #[serde(default)] exclude_names: Vec<String>,
                #[serde(default)] exclude_exts: Vec<String>,
                #[serde(default = "default_true")] detect_app_dirs: bool,
            }
            let a: Args = serde_json::from_value(args)?;
            let (tx, rx) = tokio::sync::broadcast::channel::<String>(256);
            forward_events(ctx.app_handle.clone(), rx);
            let config = ctx.config.read().clone();
            // 后台执行扫描，立即返回前端（不阻塞 invoke 通道）
            let db = ctx.db.clone();
            let app_handle = ctx.app_handle.clone();
            tokio::spawn(async move {
                let result = commands::scan::start_scan_headless(
                    db,
                    Arc::new(config),
                    a.dirs,
                    a.recursive,
                    a.exclude_dirs,
                    a.exclude_names,
                    a.exclude_exts,
                    a.detect_app_dirs,
                    tx,
                )
                .await;
                match &result {
                    Ok(_) => {},
                    Err(e) => {
                        log::error!("后台扫描错误: {}", e);
                        let _ = tauri::Emitter::emit(&app_handle, "scan_error", e.clone());
                    }
                }
            });
            Ok(Value::Null)
        }
        "scan:cancel" => {
            commands::scan::request_scan_cancel();
            Ok(Value::Null)
        }
        "enrich:cancel" => {
            commands::enrich::request_enrich_cancel();
            Ok(Value::Null)
        }
        "scan:status" => {
            // 轮询用：返回 { scanning: bool }，不查 DB
            Ok(serde_json::json!({
                "scanning": !commands::scan::is_scan_complete(),
            }))
        }
        "clean:start" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                #[serde(default)] confirm: bool,
                #[serde(default)] file_actions: Vec<Value>,
            }
            let a: Args = serde_json::from_value(args)?;
            let (tx, rx) = tokio::sync::broadcast::channel::<String>(256);
            forward_events(ctx.app_handle.clone(), rx);
            let config = ctx.config.read().clone();
            commands::clean::start_clean_headless(
                ctx.db.clone(),
                config,
                a.confirm,
                a.file_actions,
                tx,
            )
            .await?;
            Ok(Value::Null)
        }
        "enrich:start" => {
            #[derive(serde::Deserialize)]
            #[serde(rename_all = "snake_case")]
            struct Args {
                #[serde(default = "default_provider")] provider: String,
                #[serde(default = "default_concurrency")] concurrency: i32,
            }
            let a: Args = serde_json::from_value(args)?;
            let (tx, rx) = tokio::sync::broadcast::channel::<String>(256);
            forward_events(ctx.app_handle.clone(), rx);
            let config = ctx.config.read().clone();
            commands::enrich::start_enrich_headless(
                ctx.db.clone(),
                config,
                ctx.enrich_state.clone(),
                a.provider,
                a.concurrency,
                tx,
            )
            .await?;
            Ok(Value::Null)
        }
        "enrich:status" => Ok(commands::enrich::get_enrich_status_headless(&ctx.enrich_state)),

        _ => Err(PluginError::UnknownAction(action.into())),
    }
}

/// 将 headless 函数通过 broadcast 发送的事件转发到 Tauri app.emit。
///
/// headless 事件格式：`{"event":"scan_progress","data":{...}}`
/// 桥接层解析 event 字段后用原事件名 emit。
fn forward_events(app: tauri::AppHandle, mut rx: tokio::sync::broadcast::Receiver<String>) {
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            if let Ok(parsed) = serde_json::from_str::<Value>(&ev) {
                if let (Some(event), Some(data)) = (
                    parsed.get("event").and_then(|v| v.as_str()),
                    parsed.get("data"),
                ) {
                    let _ = tauri::Emitter::emit(&app, event, data.clone());
                }
            }
        }
    });
}

fn default_true() -> bool {
    true
}

fn default_task_limit() -> i64 {
    50
}

fn default_max_results() -> usize {
    100
}

fn default_provider() -> String {
    "offline".to_string()
}

fn default_concurrency() -> i32 {
    // OpenRouter free 模型限流 ~8 req/min，并发 2 让限额撑久（高并发直接 429）。
    // 用户换付费模型后可在配置调高。
    2
}
