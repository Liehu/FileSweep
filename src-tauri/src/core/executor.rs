pub use crate::core::models::{ExecutorAction, Operation};
use crate::core::models::{FileRecord, OperationLog};
use chrono::Utc;
use log::{error, info, warn};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub struct Executor {
    pub dry_run: bool,
    pub scan_dir: String,
    pub use_recycle_bin: bool,
    /// 迁移根目录：action.dest 为相对路径时拼接此根目录
    pub migrate_root: String,
}

impl Executor {
    pub fn new(dry_run: bool, scan_dir: String) -> Self {
        Self {
            dry_run,
            scan_dir,
            use_recycle_bin: true,
            migrate_root: String::new(),
        }
    }

    pub fn with_migrate_root(mut self, root: String) -> Self {
        self.migrate_root = root;
        self
    }

    /// 解析目标路径：绝对路径直接用，相对路径拼接 migrate_root。
    /// 跨平台：用 Path::join 自动处理路径分隔符（/ 或 \）。
    fn resolve_dest(&self, dest: &str) -> String {
        if dest.is_empty() {
            return String::new();
        }
        let p = Path::new(dest);
        if p.is_absolute() {
            return dest.to_string();
        }
        if self.migrate_root.is_empty() {
            return dest.to_string();
        }
        Path::new(&self.migrate_root)
            .join(dest)
            .to_string_lossy()
            .to_string()
    }

    pub fn execute(
        &self,
        actions: &[ExecutorAction],
        session_id: &str,
    ) -> Result<ExecuteResult, String> {
        let mut logs = Vec::new();

        for action in actions {
            let mut op_log = OperationLog {
                id: 0,
                timestamp: Utc::now(),
                operation: match action.operation {
                    Operation::Move => "MOVE".to_string(),
                    Operation::Delete => "DELETE".to_string(),
                    Operation::Rename => "RENAME".to_string(),
                },
                source_path: action.source.clone(),
                dest_path: action.dest.clone(),
                reason: action.reason.clone(),
                file_hash: action.file.file_hash.clone(),
                file_size: action.file.file_size,
                status: String::new(),
                session_id: session_id.to_string(),
                can_revert: false,
            };

            if self.dry_run {
                op_log.status = "dry_run".to_string();
                op_log.can_revert = false;
                info!(
                    "[DRY-RUN] operation={:?} source={}",
                    action.operation, action.source
                );
                logs.push(op_log);
                continue;
            }

            let result = match &action.operation {
                Operation::Move => {
                    let resolved_dest = self.resolve_dest(&action.dest);
                    if action.file.is_app_dir && !action.file.app_dir_path.is_empty() {
                        let dest_dir = Path::new(&resolved_dest)
                            .join(Path::new(&action.file.app_dir_path).file_name().unwrap_or_default());
                        let dest_str = dest_dir.to_string_lossy().to_string();
                        op_log.dest_path = dest_str.clone();
                        self.move_dir(&action.file.app_dir_path, &dest_str)
                    } else {
                        op_log.dest_path = resolved_dest.clone();
                        self.move_file(&action.source, &resolved_dest)
                    }
                }
                Operation::Delete => {
                    if action.file.is_app_dir && !action.file.app_dir_path.is_empty() {
                        if self.use_recycle_bin {
                            self.recycle_file(&action.file.app_dir_path)?;
                            op_log.can_revert = true;
                            Ok(())
                        } else {
                            op_log.can_revert = false;
                            self.delete_dir(&action.file.app_dir_path)
                        }
                    } else {
                        if self.use_recycle_bin {
                            self.recycle_file(&action.source)?;
                            op_log.can_revert = true;
                            Ok(())
                        } else {
                            op_log.can_revert = false;
                            self.delete_file(&action.source)
                        }
                    }
                }
                Operation::Rename => {
                    op_log.can_revert = true;
                    self.rename_file(&action.source, &action.dest)
                }
            };

            match result {
                Ok(()) => {
                    op_log.status = "success".to_string();
                }
                Err(e) => {
                    op_log.status = "error".to_string();
                    error!(
                        "执行操作失败: operation={:?} source={} error={}",
                        action.operation, action.source, e
                    );
                }
            }

            logs.push(op_log);
        }

        Ok(ExecuteResult { logs })
    }

    fn move_file(&self, src: &str, dst: &str) -> Result<(), String> {
        let dst_path = Path::new(dst);
        if let Some(parent) = dst_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目标目录失败: {}", e))?;
        }
        if fs::rename(src, dst).is_ok() {
            return Ok(());
        }
        copy_and_remove(src, dst)
    }

    fn move_dir(&self, src_dir: &str, dest_dir: &str) -> Result<(), String> {
        let dest_path = Path::new(dest_dir);
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("创建目标父目录失败: {}", e))?;
        }
        if fs::rename(src_dir, dest_dir).is_ok() {
            return Ok(());
        }
        copy_dir_recursive(src_dir, dest_dir)?;
        fs::remove_dir_all(src_dir).map_err(|e| format!("删除源目录失败: {}", e))
    }

    fn recycle_file(&self, path: &str) -> Result<(), String> {
        let abs_path = Path::new(path)
            .canonicalize()
            .unwrap_or_else(|_| Path::new(path).to_path_buf());
        let abs_str = abs_path.to_string_lossy().replace("'", "''");

        let ps_script = format!(
            "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile('{}', 'OnlyErrorDialogs', 'SendToRecycleBin')",
            abs_str
        );

        let output = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps_script])
            .output()
            .map_err(|e| format!("执行 PowerShell 失败: {}", e))?;

        if !output.status.success() {
            warn!(
                "Windows recycle bin unavailable, moving to trash dir: path={} error={}",
                abs_path.display(),
                String::from_utf8_lossy(&output.stderr)
            );
            return self.move_to_trash_dir(path);
        }
        Ok(())
    }

    fn move_to_trash_dir(&self, path: &str) -> Result<(), String> {
        let trash_dir = dirs::home_dir()
            .unwrap_or_else(|| Path::new(".").to_path_buf())
            .join(".filesweep_trash");
        fs::create_dir_all(&trash_dir)
            .map_err(|e| format!("创建回收站目录失败: {}", e))?;

        let file_name = Path::new(path).file_name().unwrap_or_default();
        let dest = trash_dir.join(file_name);

        if dest.exists() {
            let stem = Path::new(path)
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let ext = Path::new(path)
                .extension()
                .map(|e| format!(".{}", e.to_string_lossy()))
                .unwrap_or_default();
            let timestamp = Utc::now().timestamp_millis();
            let new_name = format!("{}_{}{}", stem, timestamp, ext);
            return fs::rename(path, trash_dir.join(new_name))
                .map_err(|e| format!("移动到回收站失败: {}", e));
        }

        fs::rename(path, dest).map_err(|e| format!("移动到回收站失败: {}", e))
    }

    fn delete_file(&self, path: &str) -> Result<(), String> {
        fs::remove_file(path).map_err(|e| format!("删除文件失败: {}", e))
    }

    fn delete_dir(&self, path: &str) -> Result<(), String> {
        fs::remove_dir_all(path).map_err(|e| format!("删除目录失败: {}", e))
    }

    fn rename_file(&self, old_path: &str, new_path: &str) -> Result<(), String> {
        fs::rename(old_path, new_path).map_err(|e| format!("重命名失败: {}", e))
    }
}

pub struct ExecuteResult {
    pub logs: Vec<OperationLog>,
}

fn copy_and_remove(src: &str, dst: &str) -> Result<(), String> {
    copy_single_file(src, dst)?;
    if let Err(e) = fs::remove_file(src) {
        warn!("复制完成但删除源文件失败: src={} error={}", src, e);
    }
    Ok(())
}

fn copy_dir_recursive(src_dir: &str, dest_dir: &str) -> Result<(), String> {
    fs::create_dir_all(dest_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    for entry in walkdir::WalkDir::new(src_dir).into_iter() {
        let entry = entry.map_err(|e| format!("遍历目录失败: {}", e))?;
        let path = entry.path();
        let rel = path
            .strip_prefix(src_dir)
            .map_err(|e| format!("计算相对路径失败: {}", e))?;
        let target = Path::new(dest_dir).join(rel);

        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)
                .map_err(|e| format!("创建目录失败: {}", e))?;
        } else {
            copy_single_file(&path.to_string_lossy(), &target.to_string_lossy())?;
        }
    }
    Ok(())
}

fn copy_single_file(src: &str, dst: &str) -> Result<(), String> {
    if let Some(parent) = Path::new(dst).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目标目录失败: {}", e))?;
    }
    fs::copy(src, dst).map_err(|e| format!("复制文件失败: {}", e))?;
    Ok(())
}

pub fn revert_move(src: &str, dst: &str) -> Result<(), String> {
    if !Path::new(dst).exists() {
        return Err(format!("目标文件不存在: {}", dst));
    }
    if let Some(parent) = Path::new(src).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建源目录失败: {}", e))?;
    }
    if fs::rename(dst, src).is_ok() {
        return Ok(());
    }
    copy_and_remove(dst, src)
}

pub fn revert_from_trash(trash_path: &str, original_path: &str) -> Result<(), String> {
    if !Path::new(trash_path).exists() {
        return Err(format!("回收站中文件不存在: {}", trash_path));
    }
    if let Some(parent) = Path::new(original_path).parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("创建目标目录失败: {}", e))?;
    }
    if fs::rename(trash_path, original_path).is_ok() {
        return Ok(());
    }
    copy_and_remove(trash_path, original_path)
}

pub struct Reverter<'a> {
    db: &'a crate::db::catalog::CatalogDB,
}

impl<'a> Reverter<'a> {
    pub fn new(db: &'a crate::db::catalog::CatalogDB) -> Self {
        Self { db }
    }

    pub fn revert(&mut self, log: &OperationLog) -> Result<(), String> {
        match log.operation.as_str() {
            "MOVE" => {
                // 尝试从回收站还原
                let trash_path = find_in_trash(&log.source_path);
                if !trash_path.is_empty() {
                    revert_from_trash(&trash_path, &log.source_path)?;
                } else {
                    revert_move(&log.source_path, &log.dest_path)?;
                }
                Ok(())
            }
            "DELETE" => {
                let trash_path = find_in_trash(&log.source_path);
                if !trash_path.is_empty() {
                    revert_from_trash(&trash_path, &log.source_path)?;
                } else {
                    return Err(format!("文件已在回收站中删除，无法还原: {}", log.source_path));
                }
                Ok(())
            }
            "RENAME" => {
                revert_move(&log.source_path, &log.dest_path)
            }
            _ => Err(format!("不支持的操作类型: {}", log.operation)),
        }
    }
}

pub fn find_in_trash(original_path: &str) -> String {
    let trash_dir = dirs::home_dir()
        .unwrap_or_else(|| Path::new(".").to_path_buf())
        .join(".filesweep_trash");

    let name = Path::new(original_path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let candidate = trash_dir.join(&name);
    if candidate.exists() {
        return candidate.to_string_lossy().to_string();
    }

    if let Ok(entries) = fs::read_dir(&trash_dir) {
        let ext = Path::new(original_path)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let base = Path::new(original_path)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        for entry in entries.flatten() {
            if entry.path().is_dir() {
                continue;
            }
            let n = entry.file_name().to_string_lossy().to_string();
            if !ext.is_empty() && n.len() > base.len() && n.starts_with(&base) && n.ends_with(&ext) {
                return entry.path().to_string_lossy().to_string();
            }
        }
    }

    String::new()
}
