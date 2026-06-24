//! 方案 P 执行器：复制 → 校验 → 建 junction → 删源（grill Q4c）。
//!
//! 状态机：
//!   planned → copying(checkpoint持久化每文件) → verifying → linking → deleting → done
//!   任一步失败 → failed（C: 原件完整，可 retry 续传）
//!   deleting 失败 → manual（C: 同时存在 junction + 原件，提示手动）
//!
//! 进度通过 ProgressCallback 上报（前端用 Tauri event 监听）。

use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::plugins::appmover::identify::dir_size_and_count;

/// 进度回调：当前阶段、已复制文件数、总文件数、消息。
pub type ProgressCb = Box<dyn Fn(&str, u64, u64, &str) + Send + Sync>;

/// 执行迁移。conn 用于读写 job/checkpoint，source/target 为绝对路径。
/// 成功返回 (file_count, total_bytes)。
pub fn execute_plan(
    conn: &Connection,
    job_id: i64,
    source: &str,
    target: &str,
    progress: Option<ProgressCb>,
) -> Result<(u64, u64), String> {
    let src = PathBuf::from(source);
    let dst = PathBuf::from(target);

    if !src.is_dir() {
        return Err(format!("源目录不存在: {}", source));
    }
    // 已是 junction 的情况：不应进入迁移
    if crate::plugins::appmover::identify::is_junction(&src) {
        return Err(format!("源已是 junction（可能已迁移过）: {}", source));
    }

    set_status(conn, job_id, "copying", "")?;
    let (total_bytes, total_files) = dir_size_and_count(&src);
    set_meta(conn, job_id, total_files, total_bytes)?;

    // 收集所有文件相对路径
    let files = collect_files(&src)?;
    let checkpoint = load_checkpoint(conn, job_id)?;
    let mut done_set: std::collections::HashSet<String> =
        checkpoint.iter().cloned().collect();

    // 1. 复制（带 checkpoint 续传）
    let mut copied = done_set.len() as u64;
    for (i, rel) in files.iter().enumerate() {
        if done_set.contains(rel) {
            continue;
        }
        let src_file = src.join(rel);
        let dst_file = dst.join(rel);
        if let Some(parent) = dst_file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                let msg = format!("创建目标父目录失败 {}: {}", dst_file.display(), e);
                let _ = set_status(conn, job_id, "failed", &msg);
                msg
            })?;
        }
        match std::fs::copy(&src_file, &dst_file) {
            Ok(_) => {
                done_set.insert(rel.clone());
                copied += 1;
                save_checkpoint(conn, job_id, &done_set)?;
                if let Some(cb) = &progress {
                    cb("copying", copied, total_files, &format!("复制 {}/{}", copied, total_files));
                }
            }
            Err(e) => {
                // 复制失败：保留 C: 原件，状态 failed，可续传
                let msg = format!("复制失败 {}: {}", src_file.display(), e);
                let _ = set_status(conn, job_id, "failed", &msg);
                return Err(msg);
            }
        }
        // 每 16 个文件刷新一次状态（避免过于频繁）
        if i % 16 == 0 {
            let _ = update_copied_count(conn, job_id, copied);
        }
    }
    let _ = update_copied_count(conn, job_id, copied);

    // 2. 校验：逐文件比对大小（速度优先；关键场景可加 hash）
    set_status(conn, job_id, "verifying", "")?;
    if let Some(cb) = &progress {
        cb("verifying", copied, total_files, "校验文件完整性");
    }
    verify(&src, &dst, &files).map_err(|e| {
        let _ = set_status(conn, job_id, "failed", &e);
        e
    })?;

    // 3. 建 junction：先重命名 C: 原件为 .amold，再在原位建 junction → D:，最后删 .amold
    set_status(conn, job_id, "linking", "")?;
    if let Some(cb) = &progress {
        cb("linking", total_files, total_files, "建立目录链接");
    }
    link_source_to_target(&src, &dst).map_err(|e| {
        // linking 失败：D: 完整、C: 原件也在（未删），安全，标记 failed 重跑
        let _ = set_status(conn, job_id, "failed", &e);
        e
    })?;

    // 4. 完成
    let now = chrono::Utc::now().timestamp();
    set_status_with_time(conn, job_id, "done", "", now)?;
    if let Some(cb) = &progress {
        cb("done", total_files, total_files, "迁移完成");
    }
    Ok((total_files, total_bytes))
}

/// 校验：每个文件大小一致。
fn verify(src: &Path, dst: &Path, files: &[String]) -> Result<(), String> {
    for rel in files {
        let s = src.join(rel);
        let d = dst.join(rel);
        let s_meta = std::fs::metadata(&s).map_err(|e| format!("读源元数据失败 {}: {}", s.display(), e))?;
        let d_meta = std::fs::metadata(&d).map_err(|e| format!("读目标元数据失败 {}: {}", d.display(), e))?;
        if s_meta.len() != d_meta.len() {
            return Err(format!("大小不一致: {} ({} vs {})", rel, s_meta.len(), d_meta.len()));
        }
    }
    Ok(())
}

/// 建立链接：把 C:\Foo 重命名为 C:\Foo.amold，在 C:\Foo 位置建 junction 指向 D:\Foo，
/// 然后删除 C:\Foo.amold。
/// 如果重命名后建 junction 失败，尝试把 .amold 改回原位回滚。
fn link_source_to_target(src: &Path, dst: &Path) -> Result<(), String> {
    let backup = src.with_extension("amold_backup");
    // 确保 D: 目标就绪
    if !dst.is_dir() {
        return Err(format!("目标目录不存在: {}", dst.display()));
    }
    // 重命名 C: 原件
    std::fs::rename(src, &backup).map_err(|e| format!("重命名源失败: {}", e))?;
    // 建 junction
    #[cfg(windows)]
    {
        match junction::create(src, dst) {
            Ok(_) => {}
            Err(e) => {
                // 回滚：把 backup 改回原位
                let _ = std::fs::rename(&backup, src);
                return Err(format!("建 junction 失败: {}", e));
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (src, dst);
        return Err("junction 仅支持 Windows".into());
    }
    // 删除 backup（C: 真实原件）
    if let Err(e) = remove_dir_all(&backup) {
        // 此时 C: 同时存在 junction + backup，标记 manual 由调用方处理
        return Err(format!(
            "已建链接但删除原件失败（需手动删 {}）: {}",
            backup.display(),
            e
        ));
    }
    Ok(())
}

fn remove_dir_all(path: &Path) -> std::io::Result<()> {
    // junction 目标不能用 remove_dir_all 直接删（会穿越），但 backup 是普通目录
    std::fs::remove_dir_all(path)
}

/// 递归收集目录下所有文件的相对路径（用 / 分隔）。
fn collect_files(root: &Path) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root).follow_links(false).into_iter().filter_map(|e| e.ok()) {
        if !entry.file_type().is_file() {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(out)
}

// ── DB helpers ──

fn set_status(conn: &Connection, job_id: i64, status: &str, error: &str) -> Result<(), String> {
    conn.execute(
        "UPDATE am_migrate_jobs SET status = ?1, error = ?2 WHERE id = ?3",
        rusqlite::params![status, error, job_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn set_status_with_time(
    conn: &Connection,
    job_id: i64,
    status: &str,
    error: &str,
    finished_at: i64,
) -> Result<(), String> {
    conn.execute(
        "UPDATE am_migrate_jobs SET status = ?1, error = ?2, finished_at = ?3 WHERE id = ?4",
        rusqlite::params![status, error, finished_at, job_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn set_meta(conn: &Connection, job_id: i64, file_count: u64, total_bytes: u64) -> Result<(), String> {
    conn.execute(
        "UPDATE am_migrate_jobs SET file_count = ?1, total_bytes = ?2, started_at = ?3 WHERE id = ?4",
        rusqlite::params![file_count, total_bytes, chrono::Utc::now().timestamp(), job_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn update_copied_count(conn: &Connection, job_id: i64, copied: u64) -> Result<(), String> {
    conn.execute(
        "UPDATE am_migrate_jobs SET copied_count = ?1 WHERE id = ?2",
        rusqlite::params![copied, job_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

fn load_checkpoint(conn: &Connection, job_id: i64) -> Result<Vec<String>, String> {
    let s: String = conn
        .query_row(
            "SELECT checkpoint FROM am_migrate_jobs WHERE id = ?1",
            rusqlite::params![job_id],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "[]".into());
    serde_json::from_str(&s).map_err(|e| e.to_string())
}

fn save_checkpoint(
    conn: &Connection,
    job_id: i64,
    done: &std::collections::HashSet<String>,
) -> Result<(), String> {
    let v: Vec<&String> = done.iter().collect();
    let s = serde_json::to_string(&v).map_err(|e| e.to_string())?;
    conn.execute(
        "UPDATE am_migrate_jobs SET checkpoint = ?1 WHERE id = ?2",
        rusqlite::params![s, job_id],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}
