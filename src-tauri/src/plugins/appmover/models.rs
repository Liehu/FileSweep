//! AppMover 数据模型（IPC / DB 共用）

use serde::{Deserialize, Serialize};

/// 候选迁移目录（非系统默认的一级子目录）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CandidateDir {
    pub path: String,
    pub name: String,
    /// 所在监控根（如 C:\Users\X\AppData\Roaming）
    pub watch_root: String,
    pub size_bytes: u64,
    pub file_count: u64,
    /// 是否已是 junction（已迁移过）
    pub is_junction: bool,
    /// 预填的软件描述（来自预置映射；空则前端可调 am:describe AI 补全）
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub software_name: String,
}

/// 迁移目标根映射（source_root → target_root）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetMap {
    pub source_root: String,
    pub target_root: String,
}

/// 保护集条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtectedEntry {
    pub path: String,
    pub source: String, // hardcoded | baseline | user
}

/// 迁移作业
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrateJob {
    pub id: i64,
    pub source_path: String,
    pub target_path: String,
    pub status: String, // planned | copying | verifying | linking | done | failed | manual
    #[serde(default)]
    pub checkpoint: Vec<String>,
    #[serde(default)]
    pub file_count: u64,
    #[serde(default)]
    pub copied_count: u64,
    #[serde(default)]
    pub total_bytes: u64,
    #[serde(default)]
    pub started_at: Option<i64>,
    #[serde(default)]
    pub finished_at: Option<i64>,
    #[serde(default)]
    pub error: String,
}

/// 迁移计划（执行前预演，含锁定检测）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigratePlan {
    pub source_path: String,
    pub target_path: String,
    pub size_bytes: u64,
    pub file_count: u64,
    /// 目标盘剩余空间（字节）
    pub target_free_bytes: u64,
    /// 空间是否足够（size * 1.1 <= free）
    pub space_ok: bool,
    /// 锁定检测报告
    pub locks: LockReport,
}

/// 锁定检测报告
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LockReport {
    /// 持有目录内 exe 的运行进程（进程驱动可杀）
    pub blocking_processes: Vec<ProcessInfo>,
    /// 被系统进程（explorer/dllhost）加载的目录内 DLL（需重启 explorer）
    pub shell_loaded_dlls: Vec<String>,
    /// 是否需要重启 explorer 才能解锁
    pub need_explorer_restart: bool,
    /// 是否完全可安全迁移
    pub safe: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    /// 进程 exe 路径（若可取）
    #[serde(default)]
    pub exe_path: String,
}

/// 关闭结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KillResult {
    /// 已成功关闭的进程
    pub killed: Vec<ProcessInfo>,
    /// 强杀失败需重启 explorer 解锁的 DLL
    pub need_restart_explorer: bool,
    /// 是否已重启 explorer
    pub explorer_restarted: bool,
    /// 仍无法解锁，需用户手动处理
    pub manual: Vec<String>,
    /// 全部解锁完成
    pub safe: bool,
    pub message: String,
}

/// 监控事件（一级目录新增 / 卸载残留）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorEvent {
    pub watch_root: String,
    pub dir_name: String,
    pub full_path: String,
    pub state: String, // new | resident | normal
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

/// 环境变量备份条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvBackupEntry {
    pub id: i64,
    pub scope: String,  // user | system
    pub key: String,
    pub value: String,
    pub backed_up_at: i64,
}

/// 已安装程序（Uninstall 注册表只读）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallEntry {
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub install_location: String,
    #[serde(default)]
    pub uninstall_string: String,
}

/// 软件描述（预置 + AI）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirDescription {
    pub dir_name: String,
    pub software_name: String,
    #[serde(default)]
    pub description: String,
    /// preset | ai | user
    pub source: String,
}
