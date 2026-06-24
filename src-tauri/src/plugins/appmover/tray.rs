//! 系统托盘 + 角标状态机（grill Q5 托盘降级方案）。
//!
//! 职责：
//!   - 应用启动时创建托盘图标（双击显示主窗口，右键菜单：显示/立即检查/退出）
//!   - 维护"待处理事件数"角标：monitor 发现 new/resident 时，更新 tooltip 与（如系统支持）overlay 图标
//!   - 暴露 set_badge(n) 供 monitor / actions 调用
//!
//! 注：Tauri 2 的 tray-icon API 在 Windows 上不支持原生角标数字，
//!     这里用 tooltip 文案 "[N] 待处理" + 切换图标资源表达状态。
//!     图标资源复用应用图标（无独立角标图标时的兜底）。

use std::sync::atomic::{AtomicU32, Ordering};

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Runtime,
};

/// 当前待处理事件数（monitor 轮询后更新）。
static BADGE: AtomicU32 = AtomicU32::new(0);

/// 创建托盘。在 app setup 阶段调用一次。
pub fn setup_tray<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "am_show", "显示主窗口", true, None::<&str>)?;
    let check = MenuItem::with_id(app, "am_check", "立即检查监控", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "am_quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &check, &quit])?;

    let _tray = TrayIconBuilder::with_id("appmover-tray")
        .icon(app.default_window_icon().cloned().expect("缺默认图标"))
        .tooltip("FileSweep · 软件迁移")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "am_show" => {
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
            "am_check" => {
                // 立即跑一轮监控并把事件数刷到角标
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    refresh_badge_from_monitor(&app).await;
                });
            }
            "am_quit" => {
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            // 双击托盘图标显示主窗口
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(w) = app.get_webview_window("main") {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

/// 设置角标数并更新 tooltip。
pub fn set_badge<R: Runtime>(app: &AppHandle<R>, n: u32) {
    BADGE.store(n, Ordering::SeqCst);
    let tooltip = if n == 0 {
        "FileSweep · 软件迁移".to_string()
    } else {
        format!("FileSweep · 软件迁移\n[{}] 项待处理（新增/残留目录）", n)
    };
    if let Some(tray) = app.tray_by_id("appmover-tray") {
        let _ = tray.set_tooltip(Some(&tooltip));
    }
}

/// 读取当前角标数。
pub fn current_badge() -> u32 {
    BADGE.load(Ordering::SeqCst)
}

/// 从 monitor 事件刷新角标（立即检查时调用）。
pub async fn refresh_badge_from_monitor<R: Runtime>(app: &AppHandle<R>) {
    // monitor::poll_once 需要 DB conn，放 spawn_blocking
    let app_clone = app.clone();
    let count = tauri::async_runtime::spawn_blocking(move || -> u32 {
        let state = app_clone.state::<crate::app::context::Context>();
        let ctx = state.inner();
        let db = ctx.db.clone();
        let conn = match db.conn.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        // 跑一轮检测，然后读未确认事件数
        let _ = crate::plugins::appmover::monitor::poll_once(&conn);
        crate::plugins::appmover::monitor::list_events(&conn)
            .map(|v| v.len() as u32)
            .unwrap_or(0)
    })
    .await
    .unwrap_or(0);

    set_badge(app, count);

    // 推送事件给前端，让 MonitorView 实时刷新
    use tauri::Emitter;
    let _ = app.emit("am:monitor_updated", count);
}
