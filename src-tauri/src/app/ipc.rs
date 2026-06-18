use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::app::context::Context;
use crate::app::host::PluginHost;
use crate::app::plugin::PluginMetadata;
use crate::core::config::Config;
use crate::db::catalog::CatalogDB;

/// 插件统一调用入口
/// 前端：invoke("plugin_invoke", { plugin, action, args })
#[tauri::command]
pub async fn plugin_invoke(
    plugin: String,
    action: String,
    args: Option<Value>,
    host: State<'_, Arc<PluginHost>>,
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    app_handle: tauri::AppHandle,
) -> Result<Value, String> {
    let ctx = Context {
        db: db.inner().clone(),
        config: config.inner().clone(),
        app_handle,
    };
    host.dispatch(&plugin, &action, args.unwrap_or(Value::Null), &ctx)
        .await
        .map_err(|e| e.to_string())
}

/// 列出所有插件元信息（前端命令面板/侧栏渲染用）
#[tauri::command]
pub fn plugin_list(host: State<'_, Arc<PluginHost>>) -> Vec<PluginMetadata> {
    host.metadata_list()
}
