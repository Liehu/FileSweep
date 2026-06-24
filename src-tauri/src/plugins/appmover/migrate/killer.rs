//! 三级关闭占用进程（grill Q8b/8c）。
//!
//! 策略：
//!   1. 优雅关闭：对每个 blocking_process 发 WM_CLOSE / 对进程发 CTRL_BREAK，等 5s
//!   2. 强制终止：TerminateProcess，等 2s
//!   3. 外壳占用：重启 explorer（taskkill /f explorer.exe && start explorer.exe）
//!   4. 仍失败：标记 manual
//!
//! 注：调用方应在执行前已弹窗 + 10s 倒计时确认（grill Q8c）。

use crate::plugins::appmover::models::KillResult;

/// 执行三级关闭。`force` = 是否跳过优雅直接强杀（用户已倒计时确认可传 true）。
/// `dir` 用于执行后复检是否仍被占用。
pub fn kill_locks(
    report: &crate::plugins::appmover::models::LockReport,
    force: bool,
    dir: &str,
) -> KillResult {
    let mut out = KillResult::default();

    // 1 & 2. 关闭 blocking_processes
    for p in &report.blocking_processes {
        if !force {
            graceful_close(p.pid);
            if wait_exit(p.pid, 5_000) {
                out.killed.push(p.clone());
                continue;
            }
        }
        if terminate(p.pid) && wait_exit(p.pid, 2_000) {
            out.killed.push(p.clone());
        } else {
            out.manual.push(format!("{} (pid={})", p.name, p.pid));
        }
    }

    // 3. 外壳占用：重启 explorer
    if report.need_explorer_restart {
        out.need_restart_explorer = true;
        if restart_explorer() {
            out.explorer_restarted = true;
        } else {
            out.manual.push("explorer.exe 重启失败，请手动注销/重启资源管理器".into());
        }
    }

    // 复检：目录是否仍被占用
    if !dir.is_empty() {
        let recheck = crate::plugins::appmover::migrate::locker::scan_locks(dir);
        if !recheck.safe {
            for p in recheck.blocking_processes {
                let label = format!("{} (pid={})", p.name, p.pid);
                if !out.manual.iter().any(|x| x == &label) {
                    out.manual.push(label);
                }
            }
            if recheck.need_explorer_restart && !out.explorer_restarted {
                out.manual.push("explorer 仍加载目录内 DLL，请手动注销".into());
            }
        }
    }

    out.safe = out.manual.is_empty();
    if out.safe {
        out.message = "所有占用已释放，可安全迁移".into();
    } else {
        out.message = format!("仍有 {} 项需手动处理", out.manual.len());
    }
    out
}

#[cfg(windows)]
fn graceful_close(pid: u32) {
    use winapi::shared::minwindef::LPARAM;
    use winapi::shared::windef::HWND;
    use winapi::um::winuser::{
        EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
    };
    unsafe {
        extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> i32 {
            let pid_wanted = lparam as u32;
            let mut pid: u32 = 0;
            unsafe {
                GetWindowThreadProcessId(hwnd, &mut pid);
                if pid == pid_wanted {
                    PostMessageW(hwnd, WM_CLOSE as u32, 0, 0);
                }
            }
            1
        }
        EnumWindows(Some(enum_proc), pid as LPARAM);
    }
}

#[cfg(not(windows))]
fn graceful_close(_pid: u32) {}

#[cfg(windows)]
fn terminate(pid: u32) -> bool {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{OpenProcess, TerminateProcess};
    use winapi::um::winnt::PROCESS_TERMINATE;
    unsafe {
        let h = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = TerminateProcess(h, 1) != 0;
        CloseHandle(h);
        ok
    }
}

#[cfg(not(windows))]
fn terminate(_pid: u32) -> bool {
    true
}

/// 等待进程退出（轮询 IsProcessAlive，毫秒级超时）。
fn wait_exit(pid: u32, timeout_ms: u64) -> bool {
    let start = std::time::Instant::now();
    while (start.elapsed().as_millis() as u64) < timeout_ms {
        if !is_process_alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    !is_process_alive(pid)
}

#[cfg(windows)]
fn is_process_alive(pid: u32) -> bool {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::{OpenProcess, GetExitCodeProcess};
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;
    // STILL_ACTIVE = 259（Windows 常量）
    const STILL_ACTIVE: u32 = 259;
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let mut code: u32 = 0;
        let ok = GetExitCodeProcess(h, &mut code);
        CloseHandle(h);
        ok != 0 && code == STILL_ACTIVE
    }
}

#[cfg(not(windows))]
fn is_process_alive(_pid: u32) -> bool {
    false
}

#[cfg(windows)]
fn restart_explorer() -> bool {
    use std::process::Command;
    // taskkill /f /im explorer.exe，再异步 start explorer.exe
    let kill = Command::new("taskkill")
        .args(["/F", "/IM", "explorer.exe"])
        .output();
    // explorer 被杀后会自动重启；若没自动重启则手动拉起
    std::thread::sleep(std::time::Duration::from_millis(800));
    let _ = Command::new("cmd")
        .args(["/C", "start", "explorer.exe"])
        .spawn();
    let _ = kill;
    true
}

#[cfg(not(windows))]
fn restart_explorer() -> bool {
    true
}
