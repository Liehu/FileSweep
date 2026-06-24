//! 轮询监控：监控根下"一级目录列表"的增删（grill Q6）。
//!
//! 只关心一级目录列表变化，不递归内容。
//! 新增目录 → state='new'（候选迁移）；Uninstall 注册表里已消失但目录仍在 → state='resident'（卸载残留）。
//! 周期默认 30 分钟，可配（15min / 30min / 1day）。

use std::collections::HashSet;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use rusqlite::{Connection, OptionalExtension};

use crate::app::context::Context;
use crate::plugins::appmover::identify::default_watch_roots;
use crate::plugins::appmover::models::MonitorEvent;

/// 全局监控运行标志（防止重复启动）。
static MONITOR_RUNNING: AtomicBool = AtomicBool::new(false);
/// 全局监控句柄（停止时 abort）。
static MONITOR_HANDLE: Mutex<Option<tokio::task::JoinHandle<()>>> = parking_lot::const_mutex(None);

/// 启动轮询监控。interval_secs 为轮询周期（秒）。
pub fn start(ctx: &Context, interval_secs: u64) -> Result<(), String> {
    if MONITOR_RUNNING.swap(true, Ordering::SeqCst) {
        return Err("监控已在运行".into());
    }
    let ctx = ctx.clone();
    let handle = tokio::spawn(async move {
        loop {
            // 每轮扫描 + 刷新托盘角标 + emit 给前端
            let app = ctx.app_handle.clone();
            crate::plugins::appmover::tray::refresh_badge_from_monitor(&app).await;
            // 等待下一周期，或被停止
            for _ in 0..interval_secs {
                if !MONITOR_RUNNING.load(Ordering::SeqCst) {
                    return;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    });
    *MONITOR_HANDLE.lock() = Some(handle);
    Ok(())
}

/// 停止监控。
pub fn stop() {
    MONITOR_RUNNING.store(false, Ordering::SeqCst);
    if let Some(h) = MONITOR_HANDLE.lock().take() {
        h.abort();
    }
}

pub fn is_running() -> bool {
    MONITOR_RUNNING.load(Ordering::SeqCst)
}

/// 执行一轮扫描：对比 DB 快照与磁盘实际，更新 state，返回事件。
pub fn poll_once(conn: &Connection) -> rusqlite::Result<Vec<MonitorEvent>> {
    let now = chrono::Utc::now().timestamp();
    let roots = default_watch_roots();
    let mut events = Vec::new();

    // 当前已安装软件名集合（用于判定 resident）
    let installed = installed_software_dirs();

    for root in &roots {
        let root_path = Path::new(root);
        if !root_path.is_dir() {
            continue;
        }
        let mut current: HashSet<String> = HashSet::new();
        if let Ok(entries) = std::fs::read_dir(root_path) {
            for e in entries.flatten() {
                if let Ok(ft) = e.file_type() {
                    if ft.is_dir() {
                        current.insert(e.file_name().to_string_lossy().to_string());
                    }
                }
            }
        }

        // 加载该 root 的旧快照
        let prev = load_snapshot(conn, root)?;

        // 新增
        for name in current.difference(&prev) {
            upsert_snapshot(conn, root, name, now, "new")?;
            events.push(MonitorEvent {
                watch_root: root.clone(),
                dir_name: name.clone(),
                full_path: root_path.join(name).to_string_lossy().to_string(),
                state: "new".into(),
                first_seen_at: now,
                last_seen_at: now,
            });
        }
        // 消失（卸载）
        for name in prev.difference(&current) {
            // 已从磁盘消失：若同时在 installed 里也消失 → 标记 resident（残留，但目录没了说明清理过了，跳过）
            // 这里仅更新 last_seen，不再作为事件
            mark_gone(conn, root, name, now)?;
        }

        // 当前仍在、但 installed 表里已消失的 → resident（卸载残留）
        for name in &current {
            if !installed_software_dir_match(&installed, name) {
                let cur_state = get_state(conn, root, name)?;
                if cur_state.as_deref() != Some("resident") && prev.contains(name) {
                    // 仅对"之前出现过、现在软件表里没了"的标记 resident
                    set_state(conn, root, name, "resident")?;
                    events.push(MonitorEvent {
                        watch_root: root.clone(),
                        dir_name: name.clone(),
                        full_path: root_path.join(name).to_string_lossy().to_string(),
                        state: "resident".into(),
                        first_seen_at: now,
                        last_seen_at: now,
                    });
                }
            }
        }
    }

    Ok(events)
}

/// 取所有未确认（state != normal）的事件。
pub fn list_events(conn: &Connection) -> rusqlite::Result<Vec<MonitorEvent>> {
    let mut stmt = conn.prepare(
        "SELECT watch_root, dir_name, state, first_seen_at, last_seen_at
         FROM am_monitor_snapshot
         WHERE state != 'normal'
         ORDER BY last_seen_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let watch_root: String = r.get(0)?;
        let dir_name: String = r.get(1)?;
        Ok(MonitorEvent {
            full_path: Path::new(&watch_root).join(&dir_name).to_string_lossy().to_string(),
            watch_root,
            dir_name,
            state: r.get(2)?,
            first_seen_at: r.get(3)?,
            last_seen_at: r.get(4)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// 确认/忽略某事件（标记为 normal）。
pub fn dismiss(conn: &Connection, watch_root: &str, dir_name: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE am_monitor_snapshot SET state = 'normal'
         WHERE watch_root = ?1 AND dir_name = ?2",
        rusqlite::params![watch_root, dir_name],
    )?;
    Ok(())
}

// ── 内部 DB 辅助 ──

fn load_snapshot(conn: &Connection, root: &str) -> rusqlite::Result<HashSet<String>> {
    let mut stmt = conn.prepare(
        "SELECT dir_name FROM am_monitor_snapshot WHERE watch_root = ?1",
    )?;
    let rows = stmt.query_map(rusqlite::params![root], |r| r.get::<_, String>(0))?;
    let mut set = HashSet::new();
    for r in rows {
        set.insert(r?);
    }
    Ok(set)
}

fn upsert_snapshot(
    conn: &Connection,
    root: &str,
    name: &str,
    now: i64,
    state: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO am_monitor_snapshot (watch_root, dir_name, first_seen_at, last_seen_at, state)
         VALUES (?1, ?2, ?3, ?3, ?4)
         ON CONFLICT(watch_root, dir_name) DO UPDATE SET last_seen_at = excluded.last_seen_at",
        rusqlite::params![root, name, now, state],
    )?;
    Ok(())
}

fn set_state(conn: &Connection, root: &str, name: &str, state: &str) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE am_monitor_snapshot SET state = ?3 WHERE watch_root = ?1 AND dir_name = ?2",
        rusqlite::params![root, name, state],
    )?;
    Ok(())
}

fn get_state(conn: &Connection, root: &str, name: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT state FROM am_monitor_snapshot WHERE watch_root = ?1 AND dir_name = ?2",
        rusqlite::params![root, name],
        |r| r.get::<_, String>(0),
    )
    .optional()
}

fn mark_gone(conn: &Connection, root: &str, name: &str, now: i64) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE am_monitor_snapshot SET last_seen_at = ?3, state = 'gone'
         WHERE watch_root = ?1 AND dir_name = ?2",
        rusqlite::params![root, name, now],
    )?;
    Ok(())
}

/// 取已安装软件的"目录名候选集"（来自 Uninstall 注册表 InstallLocation 的 basename）。
/// 用于判定 resident（卸载残留）。MVP 用空集合兜底（即暂不主动判 resident，仅靠 new）。
fn installed_software_dirs() -> HashSet<String> {
    #[cfg(windows)]
    {
        let mut set = HashSet::new();
        for e in crate::plugins::appmover::uninstall::list_installed() {
            if !e.install_location.is_empty() {
                if let Some(name) = Path::new(&e.install_location).file_name() {
                    set.insert(name.to_string_lossy().to_string());
                }
            }
        }
        set
    }
    #[cfg(not(windows))]
    {
        HashSet::new()
    }
}

fn installed_software_dir_match(set: &HashSet<String>, name: &str) -> bool {
    set.iter().any(|s| s.eq_ignore_ascii_case(name))
}
