pub mod app;
pub mod plugins;
pub mod core;
pub mod db;
pub mod ai;
pub mod commands;
pub mod headless;

use std::sync::Arc;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 初始化配置
    let config = core::config::default_config();
    let config_path = core::config::default_config_path();
    
    // 确保配置目录存在
    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    
    let config = if config_path.exists() {
        core::config::load_config(config_path.to_string_lossy().to_string().as_str())
            .unwrap_or(config)
    } else {
        config
    };
    let config = Arc::new(parking_lot::RwLock::new(config));

    // 初始化数据库
    if let Some(parent) = std::path::Path::new(&config.read().db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let db = db::catalog::CatalogDB::open(&config.read().db_path)
        .expect("无法打开数据库");
    db.seed_default_tags().ok();
    let db = Arc::new(db);

    // 初始化 AI 补全共享状态
    let enrich_state = Arc::new(parking_lot::Mutex::new(
        commands::enrich::EnrichState::default(),
    ));

    // 构建插件宿主
    let mut plugin_host = app::PluginHost::new();
    plugin_host.register(Box::new(plugins::FileSweepPlugin::new()));
    plugin_host.register(Box::new(plugins::AppMoverPlugin::new()));
    let plugin_host = Arc::new(plugin_host);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .manage(db.clone())
        .manage(config)
        .manage(enrich_state)
        .manage(plugin_host)
        .invoke_handler(tauri::generate_handler![
            commands::scan::start_scan,
            commands::scan::get_files,
            commands::scan::get_file_stats,
            commands::scan::get_suggestions,
            commands::clean::start_clean,
            commands::catalog::get_catalog,
            commands::catalog::update_catalog_entry,
            commands::catalog::delete_catalog_entry,
            commands::catalog::export_catalog,
            commands::enrich::start_enrich,
            commands::enrich::get_enrich_status,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::rules::get_rules,
            commands::rules::update_rules,
            commands::categories::get_func_categories,
            commands::categories::update_func_categories,
            commands::tags::get_tags,
            commands::tags::create_tag,
            commands::tags::update_tag,
            commands::tags::delete_tag,
            commands::logs::get_logs,
            commands::logs::revert_operation,
            commands::logs::batch_revert,
            commands::db_ops::reset_db,
            app::ipc::plugin_invoke,
            app::ipc::plugin_list,
        ])
        .setup(|app| {
            // 初始化 AppMover 系统托盘
            if let Err(e) = crate::plugins::appmover::tray::setup_tray(app.handle()) {
                log::warn!("AppMover tray init failed: {}", e);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running FileSweep");
}
