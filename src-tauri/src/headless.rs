//! 无头模式：通过 HTTP 服务器提供前端并桥接 Tauri IPC commands。
//!
//! 用法：filesweep --headless [--port 3210]
//! 然后在浏览器打开 http://localhost:3210

use axum::{
    Router,
    extract::{Path as AxumPath, State},
    response::{IntoResponse, Sse},
    routing::{get, post},
    Json,
};
use futures::stream::Stream;
use serde::{Deserialize};
use serde_json::{json, Value};
use std::sync::Arc;
use std::pin::Pin;
use tokio::sync::RwLock;
use tokio::sync::broadcast;
use tower_http::cors::{Any, CorsLayer};
use log::{info, error};

use crate::core::config::Config;
use crate::core::models::ScanProgress;
use crate::db::catalog::CatalogDB;
use crate::commands;

/// 无头模式共享状态
#[derive(Clone)]
pub struct HeadlessState {
    pub config: Arc<RwLock<Config>>,
    pub db: CatalogDB,
    /// 全局事件广播器，用于向浏览器推送事件
    pub event_tx: broadcast::Sender<String>,
}

/// Headless CLI 参数
#[derive(Debug, Clone)]
pub struct HeadlessOptions {
    pub port: u16,
    pub host: String,
}

impl Default for HeadlessOptions {
    fn default() -> Self {
        Self {
            port: 3210,
            host: "0.0.0.0".to_string(),
        }
    }
}

/// 启动无头模式 HTTP 服务器
pub async fn run_headless(config: Arc<Config>, db: CatalogDB, opts: HeadlessOptions) {
    let (event_tx, _) = broadcast::channel::<String>(256);
    let state = HeadlessState { config: Arc::new(RwLock::new((*config).clone())), db, event_tx };

    let dist_dir = std::path::PathBuf::from("dist");
    let serve_dir = if dist_dir.exists() {
        tower_http::services::ServeDir::new(&dist_dir)
    } else {
        // dist 目录不存在，使用 fallback
        tower_http::services::ServeDir::new(".")
    };

    let app = Router::new()
        // API 路由 - 代理 Tauri commands（优先匹配）
        .route("/api/invoke/{cmd}", post(invoke_command))
        .route("/api/events", get(events_stream))
        // 静态前端文件（从 dist 目录提供）
        .fallback_service(serve_dir)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .with_state(Arc::new(state));

    let addr = format!("{}:{}", opts.host, opts.port);
    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            error!("绑定端口 {} 失败: {}", addr, e);
            std::process::exit(1);
        }
    };

    info!("=============================================");
    info!("  FileSweep 无头模式已启动");
    info!("  浏览器访问: http://localhost:{}", opts.port);
    info!("=============================================");

    if let Err(e) = axum::serve(listener, app).await {
        error!("服务器运行错误: {}", e);
    }
}

/// 通用的 IPC invoke 代理
///
/// 前端调用 POST /api/invoke/<command_name>，body 为 JSON 参数。
/// 此函数解析命令名并调用对应的后端处理函数。
#[derive(Deserialize)]
struct InvokeParams {
    // 允许任意 JSON 参数，每个命令自行提取
}

async fn invoke_command(
    State(state): State<Arc<HeadlessState>>,
    AxumPath(cmd): AxumPath<String>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let result = match cmd.as_str() {
        // ── 扫描 ──
        "start_scan" => {
            // P2: start_scan_headless 签名改为 Arc<CatalogDB>，headless 模式适配留 P3
            Err("start_scan in headless mode: 适配 P3（start_scan_headless 签名变更）".to_string())
        }

        "get_files" => {
            let page = body.get("page").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
            let page_size = body.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(20) as i32;
            let category = body.get("category").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let search = body.get("search").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let dir_type = body.get("dirType").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let task_id = body.get("taskId").and_then(|v| v.as_str()).unwrap_or("").to_string();
            commands::scan::get_files_headless(
                &state.db, page, page_size,
                Some(category).filter(|s| !s.is_empty()),
                Some(status).filter(|s| !s.is_empty()),
                Some(search).filter(|s| !s.is_empty()),
                Some(dir_type).filter(|s| !s.is_empty()),
                Some(task_id).filter(|s| !s.is_empty()),
            )
        }

        "get_file_stats" => {
            commands::scan::get_file_stats_headless(&state.db)
        }

        "get_suggestions" => {
            commands::scan::get_suggestions_headless(&state.db, &state.config).await
        }

        // ── 设置 ──
        "get_settings" => {
            commands::settings::get_settings_headless(&state.config).await
        }

        "update_settings" => {
            commands::settings::update_settings_headless(
                &state.config, body.clone()
            ).await
        }

        "get_rules" => {
            commands::rules::get_rules_headless(&state.config).await
        }

        "update_rules" => {
            commands::rules::update_rules_headless(&state.config, body.clone()).await
        }

        // ── 分类 ──
        "get_func_categories" => {
            commands::categories::get_func_categories_headless(&state.config).await
        }

        "update_func_categories" => {
            commands::categories::update_func_categories_headless(&state.config, body.clone()).await
        }

        // ── 标签 ──
        "get_tags" => {
            commands::tags::get_tags_headless(&state.db)
        }

        "create_tag" => {
            let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let color = body.get("color").and_then(|v| v.as_str()).unwrap_or("#3b82f6").to_string();
            let description = body.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
            commands::tags::create_tag_headless(&state.db, name, color, description)
        }

        "update_tag" => {
            commands::tags::update_tag_headless(&state.db, body.clone())
        }

        "delete_tag" => {
            let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            commands::tags::delete_tag_headless(&state.db, id)
        }

        // ── 目录 ──
        "get_catalog" => {
            let page = body.get("page").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
            let page_size = body.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(20) as i32;
            let search = body.get("search").and_then(|v| v.as_str()).unwrap_or("").to_string();
            commands::catalog::get_catalog_headless(
                &state.db, page, page_size,
                Some(search).filter(|s| !s.is_empty()),
            )
        }

        "update_catalog_entry" => {
            commands::catalog::update_catalog_entry_headless(&state.db, body.clone())
        }

        "delete_catalog_entry" => {
            let id = body.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            commands::catalog::delete_catalog_entry_headless(&state.db, id)
        }

        "export_catalog" => {
            let format = body.get("format").and_then(|v| v.as_str()).unwrap_or("csv").to_string();
            commands::catalog::export_catalog_headless(&state.db, &format)
        }

        // ── 日志 ──
        "get_logs" => {
            let page = body.get("page").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
            let page_size = body.get("pageSize").and_then(|v| v.as_i64()).unwrap_or(1000) as i32;
            let action = body.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let status = body.get("status").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let q = body.get("q").and_then(|v| v.as_str()).unwrap_or("").to_string();
            commands::logs::get_logs_headless(
                &state.db, page, page_size,
                Some(action).filter(|s| !s.is_empty()),
                Some(status).filter(|s| !s.is_empty()),
                Some(q).filter(|s| !s.is_empty()),
            )
        }

        "revert_operation" => {
            let id = body.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
            commands::logs::revert_operation_headless(&state.db, id)
        }

        "batch_revert" => {
            let ids: Vec<i64> = body.get("ids")
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_i64()).collect())
                .unwrap_or_default();
            commands::logs::batch_revert_headless(&state.db, ids)
        }

        // ── 数据库操作 ──
        "reset_db" => {
            commands::db_ops::reset_db_headless(&state.db)
        }

        // ── 丰富 ──
        "start_enrich" => {
            // P2: start_enrich_headless 签名变更，headless 模式适配留 P3
            Err("start_enrich in headless mode: 适配 P3".to_string())
        }

        _ => {
            Err(format!("未知命令: {}", cmd))
        }
    };

    match result {
        Ok(value) => Json(json!({ "ok": true, "data": value })),
        Err(e) => Json(json!({ "ok": false, "error": e })),
    }
}

/// SSE 事件流：订阅全局事件广播器，推送到浏览器
async fn events_stream(
    State(state): State<Arc<HeadlessState>>,
) -> Sse<impl Stream<Item = Result<axum::response::sse::Event, std::convert::Infallible>>> {
    let rx = state.event_tx.subscribe();

    let stream = async_stream::try_stream! {
        let data = serde_json::json!({"type":"connected"}).to_string();
        yield axum::response::sse::Event::default().data(data);

        let mut rx = rx;
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    yield axum::response::sse::Event::default().data(msg);
                }
                Err(broadcast::error::RecvError::Lagged(_n)) => {
                    continue;
                }
                Err(broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    };

    Sse::new(stream)
}

/// 将 ScanProgress 序列化为 JSON 并通过广播器发送
fn emit_event(tx: &broadcast::Sender<String>, event_name: &str, payload: &ScanProgress) {
    let data = serde_json::to_string(payload).unwrap_or_default();
    let _ = tx.send(format!("{{\"event\":\"{}\",\"data\":{}}}", event_name, data));
}

fn emit_json_event(tx: &broadcast::Sender<String>, event_name: &str, data: Value) {
    let _ = tx.send(format!("{{\"event\":\"{}\",\"data\":{}}}", event_name, data));
}

fn extract_string_array(body: &Value, key: &str) -> Vec<String> {
    body.get(key)
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default()
}
