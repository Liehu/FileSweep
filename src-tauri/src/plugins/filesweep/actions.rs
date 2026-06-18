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
            Ok(commands::scan::get_files_headless(
                &ctx.db,
                a.page.unwrap_or(1),
                a.page_size.unwrap_or(50),
                a.category,
                a.status,
                a.search,
            )?)
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

        // ═════════ 未实现（P2 补齐，双轨并存下可用旧 invoke）═════════
        "scan:start" | "clean:start" | "enrich:start" | "enrich:status" => {
            Err(PluginError::Internal(format!(
                "action '{}' 暂未在 filesweep 插件实现（P2 补齐）。当前请通过旧 invoke 命令调用。",
                action
            )))
        }

        _ => Err(PluginError::UnknownAction(action.into())),
    }
}
