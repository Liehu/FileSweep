//! 占用检测：扫描目录被哪些进程/DLL 锁定（grill Q8a）。
//!
//! 两类占用：
//!   (i) 进程持有句柄 —— 进程 exe 路径落在待迁目录内 → 进程驱动可杀
//!   (ii) 目录内 DLL 被系统外壳进程（explorer/dllhost）加载 → 需重启 explorer
//!
//! 实现策略（Windows，纯 std + winapi，避免 RestartManager 的 COM 复杂性）：
//!   - 枚举所有进程，取其 exe 全路径，判断是否以 dir 为前缀 → (i)
//!   - 对外壳进程，枚举其已加载模块（Module32），判断 DLL 路径是否落在 dir 内 → (ii)
//!
//! 非 Windows 平台返回空报告。

use crate::plugins::appmover::models::{LockReport, ProcessInfo};

/// 扫描目录的占用情况。
pub fn scan_locks(dir: &str) -> LockReport {
    let _ = dir;
    #[cfg(windows)]
    return scan_locks_windows(dir);
    #[cfg(not(windows))]
    return LockReport {
        safe: true,
        ..Default::default()
    };
}

#[cfg(windows)]
fn scan_locks_windows(dir: &str) -> LockReport {
    let dir_norm = normalize(dir);
    let procs = list_processes();
    let mut blocking = Vec::new();
    let mut shell_loaded_dlls = Vec::new();
    let mut need_explorer_restart = false;

    for p in &procs {
        if p.exe_path.is_empty() {
            continue;
        }
        let exe_norm = normalize(&p.exe_path);
        // (i) 进程 exe 落在 dir 内
        if exe_norm.starts_with(&dir_norm) {
            blocking.push(p.clone());
        }
        // (ii) 外壳进程加载了 dir 内的 DLL
        if is_shell_process(&p.name) {
            if let Some(dlls) = loaded_modules_under(p.pid, &dir_norm) {
                if !dlls.is_empty() {
                    need_explorer_restart = true;
                    for d in dlls {
                        if !shell_loaded_dlls.iter().any(|x: &String| x == &d) {
                            shell_loaded_dlls.push(d);
                        }
                    }
                }
            }
        }
    }

    let safe = blocking.is_empty() && !need_explorer_restart;
    LockReport {
        blocking_processes: blocking,
        shell_loaded_dlls,
        need_explorer_restart: need_explorer_restart,
        safe,
    }
}

#[cfg(windows)]
fn is_shell_process(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n == "explorer.exe" || n == "dllhost.exe" || n == "sihost.exe" || n == "taskhostw.exe"
}

#[cfg(windows)]
fn normalize(p: &str) -> String {
    let mut s = p.replace('/', "\\");
    if !s.ends_with('\\') {
        s.push('\\');
    }
    s.to_ascii_lowercase()
}

/// 枚举所有进程（pid / name / exe_path）。
#[cfg(windows)]
fn list_processes() -> Vec<ProcessInfo> {
    use std::mem::MaybeUninit;
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;

    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap.is_null() {
            return out;
        }
        let mut entry: PROCESSENTRY32W = MaybeUninit::zeroed().assume_init();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        if Process32FirstW(snap, &mut entry) != 0 {
            loop {
                let name = String::from_utf16_lossy(
                    &entry.szExeFile[..entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(0)],
                );
                let pid = entry.th32ProcessID;
                let exe_path = query_exe_path(pid);
                out.push(ProcessInfo {
                    pid,
                    name,
                    exe_path,
                });
                if Process32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
    }
    out
}

#[cfg(windows)]
fn query_exe_path(pid: u32) -> String {
    use winapi::um::handleapi::CloseHandle;
    use winapi::um::processthreadsapi::OpenProcess;
    use winapi::um::winnt::PROCESS_QUERY_LIMITED_INFORMATION;
    use winapi::um::psapi::{GetModuleFileNameExW, K32GetModuleFileNameExW};

    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return String::new();
        }
        let mut buf = [0u16; 1024];
        // GetModuleFileNameExW 与 K32GetModuleFileNameExW 同义，优先用符号
        let len = GetModuleFileNameExW(h, std::ptr::null_mut(), buf.as_mut_ptr(), buf.len() as u32);
        CloseHandle(h);
        if len == 0 {
            return String::new();
        }
        String::from_utf16_lossy(&buf[..len as usize])
    }
}

/// 枚举外壳进程加载的模块，返回路径落在 dir 内的 DLL 列表。
#[cfg(windows)]
fn loaded_modules_under(pid: u32, dir_norm: &str) -> Option<Vec<String>> {
    use std::mem::MaybeUninit;
    use winapi::um::handleapi::{CloseHandle, INVALID_HANDLE_VALUE};
    use winapi::um::tlhelp32::{
        CreateToolhelp32Snapshot, Module32FirstW, Module32NextW, MODULEENTRY32W, TH32CS_SNAPMODULE,
        TH32CS_SNAPMODULE32,
    };

    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32, pid);
        if snap == INVALID_HANDLE_VALUE {
            return None;
        }
        let mut entry: MODULEENTRY32W = MaybeUninit::zeroed().assume_init();
        entry.dwSize = std::mem::size_of::<MODULEENTRY32W>() as u32;
        let mut hits = Vec::new();
        if Module32FirstW(snap, &mut entry) != 0 {
            loop {
                let path = String::from_utf16_lossy(
                    &entry.szExePath[..entry.szExePath.iter().position(|&c| c == 0).unwrap_or(0)],
                );
                let pn = normalize(&path);
                if pn.starts_with(dir_norm) {
                    hits.push(path);
                }
                if Module32NextW(snap, &mut entry) == 0 {
                    break;
                }
            }
        }
        CloseHandle(snap);
        Some(hits)
    }
}
