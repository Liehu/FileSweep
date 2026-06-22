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

/// filesweep 插件 action 分发
pub async fn dispatch(action: &str, args: Value, ctx: &Context) -> Result<Value, PluginError> {
    match action {
        // ═════════ scan ═════════
        "scan:stats" => {
            let v = commands::scan::get_file_stats_headless(&ctx.db)?;
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
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let page = a.page.unwrap_or(1);
            let page_size = a.page_size.unwrap_or(50);
            let category = a.category;
            let status = a.status;
            let search = a.search;
            // 用 spawn_blocking 避免阻塞 tokio runtime
            let result = tokio::task::spawn_blocking(move || {
                commands::scan::get_files_headless(
                    &db, page, page_size, category, status, search,
                )
            })
            .await
            .map_err(|e| format!("spawn_blocking 失败: {}", e))?;
            Ok(result?)
        }
        "scan:suggestions" => {
            // headless 版本需要 Arc<tokio::sync::RwLock<Config>>，从 Context 读出 Config 值包装
            let cfg = ctx.config.read().clone();
            let tok_cfg = Arc::new(tokio::sync::RwLock::new(cfg));
            Ok(commands::scan::get_suggestions_headless(&ctx.db, &tok_cfg).await?)
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
            Ok(commands::settings::update_settings_headless(&tok_cfg, args).await?)
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
                    Ok(_) => log::info!("[scan:start] 后台扫描完成"),
                    Err(e) => {
                        log::error!("[scan:start] 后台扫描错误: {}", e);
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

fn default_provider() -> String {
    "offline".to_string()
}

fn default_concurrency() -> i32 {
    4
}
