//! AppMover 插件 action 分发。

use serde_json::{json, Value};

use crate::app::context::Context;
use crate::app::plugin::PluginError;
use crate::plugins::appmover::baseline;
use crate::plugins::appmover::describe;
use crate::plugins::appmover::envvar;
use crate::plugins::appmover::identify;
use crate::plugins::appmover::migrate;
use crate::plugins::appmover::models::{MigrateJob, TargetMap};
use crate::plugins::appmover::monitor;
use crate::plugins::appmover::uninstall;

pub async fn dispatch(action: &str, args: Value, ctx: &Context) -> Result<Value, PluginError> {
    match action {
        // ═════════ 识别 ═════════
        "am:scan_candidates" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] roots: Option<Vec<String>>,
            }
            let a: Args = serde_json::from_value(args).unwrap_or(Args { roots: None });
            let db = ctx.db.clone();
            let roots = a.roots;
            let v = tokio::task::spawn_blocking(move || {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                // 为每个候选补描述
                let mut cands = identify::scan_candidates(&conn, roots.as_deref())?;
                for c in cands.iter_mut() {
                    let d = describe::describe(&conn, &c.name);
                    c.software_name = d.software_name;
                    c.description = d.description;
                }
                Ok::<_, String>(cands)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(v))
        }
        "am:describe" => {
            #[derive(serde::Deserialize)]
            struct Args { dir_name: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let dir_name = a.dir_name.clone();
            // 先 DB / 预置
            let d = {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                describe::describe(&conn, &a.dir_name)
            };
            // 若是兜底，尝试 AI（best-effort）
            let d = if d.source == "fallback" {
                if let Some(ai_d) = describe::describe_with_ai(&dir_name).await {
                    ai_d
                } else {
                    d
                }
            } else {
                d
            };
            Ok(json!(d))
        }
        "am:describe_update" => {
            #[derive(serde::Deserialize)]
            struct Args {
                dir_name: String,
                software_name: String,
                #[serde(default)] description: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                describe::upsert(&conn, &a.dir_name, &a.software_name, &a.description)
                    .map_err(|e| e.to_string())?;
                Ok::<_, String>(())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"ok": true}))
        }
        "am:list_describe" => {
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                Ok::<_, String>(describe::list_all(&conn))
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(v))
        }

        // ═════════ 基线 / 保护集 ═════════
        "am:import_baseline" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] file_path: Option<String>,
                #[serde(default)] names: Option<Vec<String>>,
            }
            let a: Args = serde_json::from_value(args).unwrap_or(Args {
                file_path: None,
                names: None,
            });
            let db = ctx.db.clone();
            let count = tokio::task::spawn_blocking(move || -> Result<usize, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                baseline::seed_hard_whitelist(&conn).map_err(|e| e.to_string())?;
                if let Some(fp) = a.file_path {
                    baseline::import_baseline_file(&conn, &fp)
                } else if let Some(names) = a.names {
                    baseline::import_baseline_names(&conn, &names)
                } else {
                    Err("需要 file_path 或 names 参数".into())
                }
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"imported": count}))
        }
        "am:set_first_scan_as_baseline" => {
            let db = ctx.db.clone();
            let count = tokio::task::spawn_blocking(move || -> Result<usize, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                baseline::seed_hard_whitelist(&conn).map_err(|e| e.to_string())?;
                let roots = identify::default_watch_roots();
                let mut names = Vec::new();
                for root in &roots {
                    let p = std::path::Path::new(root);
                    if !p.is_dir() {
                        continue;
                    }
                    if let Ok(entries) = std::fs::read_dir(p) {
                        for e in entries.flatten() {
                            if let Ok(ft) = e.file_type() {
                                if ft.is_dir() {
                                    names.push(e.file_name().to_string_lossy().to_string());
                                }
                            }
                        }
                    }
                }
                baseline::import_baseline_names(&conn, &names)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"imported": count}))
        }
        "am:get_protected" => {
            let db = ctx.db.clone();
            let v = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                baseline::seed_hard_whitelist(&conn).map_err(|e| e.to_string())?;
                Ok(baseline::list_protected(&conn).map_err(|e| e.to_string())?)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(v))
        }
        "am:add_protected" => {
            #[derive(serde::Deserialize)]
            struct Args { name: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                baseline::add_protected(&conn, &a.name).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"ok": true}))
        }
        "am:remove_protected" => {
            #[derive(serde::Deserialize)]
            struct Args { name: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                baseline::remove_protected(&conn, &a.name).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"ok": true}))
        }

        // ═════════ 目标根映射 ═════════
        "am:get_target_map" => {
            let db = ctx.db.clone();
            let v: Vec<TargetMap> = tokio::task::spawn_blocking(move || -> Result<Vec<TargetMap>, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                migrate::planner::list_target_map(&conn).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(v))
        }
        "am:set_target_map" => {
            #[derive(serde::Deserialize)]
            struct Args {
                source_root: String,
                target_root: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                migrate::planner::set_target_map(&conn, &a.source_root, &a.target_root)
                    .map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"ok": true}))
        }
        "am:remove_target_map" => {
            #[derive(serde::Deserialize)]
            struct Args { source_root: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                migrate::planner::remove_target_map(&conn, &a.source_root).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"ok": true}))
        }

        // ═════════ 迁移 ═════════
        "am:plan_migration" => {
            #[derive(serde::Deserialize)]
            struct Args { source_path: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let plan = tokio::task::spawn_blocking(move || -> Result<_, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                migrate::planner::build_plan(&conn, &a.source_path)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(plan))
        }
        "am:scan_locks" => {
            #[derive(serde::Deserialize)]
            struct Args { dir: String }
            let a: Args = serde_json::from_value(args)?;
            let dir = a.dir.clone();
            let report = tokio::task::spawn_blocking(move || migrate::locker::scan_locks(&dir))
                .await
                .map_err(|e| format!("spawn_blocking: {}", e))?;
            Ok(json!(report))
        }
        "am:kill_locks" => {
            #[derive(serde::Deserialize)]
            struct Args {
                dir: String,
                #[serde(default)] force: bool,
            }
            let a: Args = serde_json::from_value(args)?;
            // 重新扫描得到最新 lock report
            let dir = a.dir.clone();
            let force = a.force;
            let result = tokio::task::spawn_blocking(move || {
                let report = migrate::locker::scan_locks(&dir);
                migrate::killer::kill_locks(&report, force, &dir)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))?;
            Ok(json!(result))
        }
        "am:execute_migration" => {
            #[derive(serde::Deserialize)]
            struct Args {
                source_path: String,
                #[serde(default)] target_path: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let app_handle = ctx.app_handle.clone();
            let res: (i64, u64, u64) = tokio::task::spawn_blocking(move || -> Result<(i64, u64, u64), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                // 解析 target
                let target = match a.target_path {
                    Some(t) => t,
                    None => {
                        let map = migrate::planner::list_target_map(&conn).map_err(|e| e.to_string())?;
                        migrate::planner::resolve_target(&map, &a.source_path)
                            .ok_or_else(|| "未配置源根映射".to_string())?
                    }
                };
                // 创建 job
                conn.execute(
                    "INSERT INTO am_migrate_jobs (source_path, target_path, status) VALUES (?1, ?2, 'planned')",
                    rusqlite::params![a.source_path, target],
                )
                .map_err(|e| e.to_string())?;
                let job_id = conn.query_row(
                    "SELECT last_insert_rowid()",
                    [],
                    |r| r.get::<_, i64>(0),
                ).map_err(|e| e.to_string())?;

                let handle = app_handle;
                let progress: migrate::copier::ProgressCb = Box::new(move |stage, copied, total, msg| {
                    use tauri::Emitter;
                    let _ = handle.emit(
                        "am:migrate_progress",
                        json!({
                            "job_id": job_id,
                            "stage": stage,
                            "copied": copied,
                            "total": total,
                            "message": msg,
                        }),
                    );
                });
                let (files, bytes) = migrate::copier::execute_plan(
                    &conn, job_id, &a.source_path, &target, Some(progress),
                )?;
                Ok((job_id, files, bytes))
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"job_id": res.0, "file_count": res.1, "total_bytes": res.2}))
        }
        "am:retry_migration" => {
            #[derive(serde::Deserialize)]
            struct Args { job_id: i64 }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let res: (u64, u64) = tokio::task::spawn_blocking(move || -> Result<(u64, u64), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                let (source, target): (String, String) = conn
                    .query_row(
                        "SELECT source_path, target_path FROM am_migrate_jobs WHERE id = ?1",
                        rusqlite::params![a.job_id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(|e| e.to_string())?;
                // 若已完成（junction 已建），不允许重试
                let status: String = conn
                    .query_row(
                        "SELECT status FROM am_migrate_jobs WHERE id = ?1",
                        rusqlite::params![a.job_id],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                if status == "done" {
                    return Err("任务已完成，无法重试".into());
                }
                // 若 linking 已成功（C: 已是 junction），重试只做删除原件；MVP 简化为拒绝
                migrate::copier::execute_plan(&conn, a.job_id, &source, &target, None)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"file_count": res.0, "total_bytes": res.1}))
        }
        "am:list_jobs" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] limit: Option<i64>,
            }
            let a: Args = serde_json::from_value(args).unwrap_or(Args { limit: None });
            let limit = a.limit.unwrap_or(100);
            let db = ctx.db.clone();
            let jobs: Vec<MigrateJob> = tokio::task::spawn_blocking(move || -> Result<Vec<MigrateJob>, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                let mut stmt = conn
                    .prepare(
                        "SELECT id, source_path, target_path, status, checkpoint, file_count,
                                copied_count, total_bytes, started_at, finished_at, error
                         FROM am_migrate_jobs ORDER BY id DESC LIMIT ?1",
                    )
                    .map_err(|e| e.to_string())?;
                let rows = stmt
                    .query_map(rusqlite::params![limit], |r| {
                        let cp: String = r.get(4)?;
                        let checkpoint: Vec<String> = serde_json::from_str(&cp).unwrap_or_default();
                        Ok(MigrateJob {
                            id: r.get(0)?,
                            source_path: r.get(1)?,
                            target_path: r.get(2)?,
                            status: r.get(3)?,
                            checkpoint,
                            file_count: r.get(5)?,
                            copied_count: r.get(6)?,
                            total_bytes: r.get(7)?,
                            started_at: r.get(8)?,
                            finished_at: r.get(9)?,
                            error: r.get(10)?,
                        })
                    })
                    .map_err(|e| e.to_string())?;
                let mut out = Vec::new();
                for row in rows {
                    out.push(row.map_err(|e| e.to_string())?);
                }
                Ok(out)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(jobs))
        }

        // ═════════ 监控 ═════════
        "am:start_monitor" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] interval_secs: Option<u64>,
            }
            let a: Args = serde_json::from_value(args).unwrap_or(Args { interval_secs: None });
            let interval = a.interval_secs.unwrap_or(30 * 60);
            monitor::start(ctx, interval)?;
            Ok(json!({"running": true, "interval_secs": interval}))
        }
        "am:stop_monitor" => {
            monitor::stop();
            Ok(json!({"running": false}))
        }
        "am:get_monitor_events" => {
            let db = ctx.db.clone();
            let events = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                monitor::list_events(&conn).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({
                "events": events,
                "running": monitor::is_running(),
            }))
        }
        "am:dismiss_event" => {
            #[derive(serde::Deserialize)]
            struct Args {
                watch_root: String,
                dir_name: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            tokio::task::spawn_blocking(move || -> Result<(), String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                monitor::dismiss(&conn, &a.watch_root, &a.dir_name).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"ok": true}))
        }

        // ═════════ 环境变量 / 卸载表 ═════════
        "am:backup_env" => {
            #[derive(serde::Deserialize)]
            struct Args { scope: String }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let count = tokio::task::spawn_blocking(move || -> Result<usize, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                envvar::backup_env(&conn, &a.scope)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"backed_up": count}))
        }
        "am:restore_env" => {
            #[derive(serde::Deserialize)]
            struct Args {
                scope: String,
                backed_up_at: i64,
            }
            let a: Args = serde_json::from_value(args)?;
            let db = ctx.db.clone();
            let count = tokio::task::spawn_blocking(move || -> Result<usize, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                envvar::restore_env(&conn, &a.scope, a.backed_up_at)
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!({"restored": count}))
        }
        "am:list_env_backups" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] scope: Option<String>,
            }
            let a: Args = serde_json::from_value(args).unwrap_or(Args { scope: None });
            let db = ctx.db.clone();
            let rows = tokio::task::spawn_blocking(move || -> Result<Vec<_>, String> {
                let conn = db.conn.lock().map_err(|e| e.to_string())?;
                envvar::list_backups(&conn, a.scope.as_deref()).map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("spawn_blocking: {}", e))??;
            Ok(json!(rows))
        }
        "am:list_installed" => {
            let entries = tokio::task::spawn_blocking(uninstall::list_installed)
                .await
                .map_err(|e| format!("spawn_blocking: {}", e))?;
            Ok(json!(entries))
        }

        // ═════════ 托盘 / 自启 ═════════
        "am:get_badge" => {
            Ok(json!({ "count": crate::plugins::appmover::tray::current_badge() }))
        }
        "am:refresh_badge" => {
            let app = ctx.app_handle.clone();
            crate::plugins::appmover::tray::refresh_badge_from_monitor(&app).await;
            Ok(json!({ "count": crate::plugins::appmover::tray::current_badge() }))
        }
        "am:get_autostart" => {
            use tauri_plugin_autostart::ManagerExt;
            let enabled = ctx.app_handle.autolaunch().is_enabled().unwrap_or(false);
            Ok(json!({ "enabled": enabled }))
        }
        "am:set_autostart" => {
            #[derive(serde::Deserialize)]
            struct Args { enabled: bool }
            let a: Args = serde_json::from_value(args)?;
            use tauri_plugin_autostart::ManagerExt;
            let mgr = ctx.app_handle.autolaunch();
            let res = if a.enabled {
                mgr.enable()
            } else {
                mgr.disable()
            };
            res.map_err(|e| format!("切换自启失败: {}", e))?;
            Ok(json!({ "enabled": a.enabled }))
        }

        _ => Err(PluginError::UnknownAction(action.into())),
    }
}
