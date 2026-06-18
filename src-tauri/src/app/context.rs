use std::sync::Arc;
use parking_lot::RwLock;

use crate::core::config::Config;
use crate::db::catalog::CatalogDB;

/// 插件运行时上下文：提供对共享状态的访问
///
/// 复用 lib.rs 已 manage 的 db / config，插件无需自行初始化。
/// CatalogDB 内含 Mutex<Connection>，不可 Clone，故用 Arc 共享。
#[derive(Clone)]
pub struct Context {
    pub db: Arc<CatalogDB>,
    pub config: Arc<RwLock<Config>>,
    pub app_handle: tauri::AppHandle,
}
