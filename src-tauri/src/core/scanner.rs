use crate::core::appdir::{self, compute_dir_hash, compute_dir_size};
use crate::core::models::{FileRecord, ScanProgress};
use crate::core::version::extract_version;
use chrono::Utc;
use log::warn;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

pub struct Scanner {
    workers: usize,
}

struct ScanEntry {
    path: PathBuf,
    size: u64,
    mod_time: chrono::DateTime<chrono::Utc>,
    is_app_dir: bool,
    app_dir_path: PathBuf,
    app_dir_sig: appdir::AppDirSignature,
}

impl Scanner {
    pub fn new() -> Self {
        let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        Self { workers }
    }

    pub async fn scan(
        &self,
        dir: &str,
        recursive: bool,
        detect_app_dirs: bool,
        progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>,
    ) -> Result<Vec<FileRecord>, String> {
        let abs_dir = fs::canonicalize(dir).map_err(|e| format!("解析路径失败: {}", e))?;
        let entries = self.walk_dir(&abs_dir, recursive, detect_app_dirs)?;
        let records = self.hash_files(entries, &abs_dir, progress_tx).await;
        Ok(records)
    }

    fn walk_dir(
        &self,
        dir: &Path,
        recursive: bool,
        detect_app_dirs: bool,
    ) -> Result<Vec<ScanEntry>, String> {
        let mut entries = Vec::new();
        let mut app_dir_paths = std::collections::HashSet::new();

        for entry in walkdir::WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
        {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    warn!("跳过无法访问的路径: {}", e);
                    continue;
                }
            };

            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy();

            // Skip hidden files/dirs
            if file_name.starts_with('.') {
                if entry.file_type().is_dir() {
                    continue; // walkdir will still recurse; we handle it below
                }
                continue;
            }

            if entry.file_type().is_dir() {
                if !recursive && path != dir {
                    continue;
                }
                // AppDir detection on non-root subdirectories
                if detect_app_dirs && path != dir {
                    let sig = appdir::detect_app_dir(path);
                    if sig.is_app_dir {
                        let metadata = match fs::metadata(path) {
                            Ok(m) => m,
                            Err(_) => {
                                app_dir_paths.insert(path.to_path_buf());
                                entries.push(ScanEntry {
                                    path: path.to_path_buf(),
                                    size: 0,
                                    mod_time: std::time::SystemTime::now().into(),
                                    is_app_dir: true,
                                    app_dir_path: path.to_path_buf(),
                                    app_dir_sig: sig,
                                });
                                continue;
                            }
                        };
                        app_dir_paths.insert(path.to_path_buf());
                        entries.push(ScanEntry {
                            path: path.to_path_buf(),
                            size: 0,
                            mod_time: metadata.modified().unwrap_or_else(|_| std::time::SystemTime::now()).into(),
                            is_app_dir: true,
                            app_dir_path: path.to_path_buf(),
                            app_dir_sig: sig,
                        });
                        continue;
                    }
                }
                continue;
            }

            // Skip files inside detected app dirs
            if let Some(parent) = path.parent() {
                if app_dir_paths.contains(parent) {
                    continue;
                }
            }

            // Skip symlinks
            if entry.file_type().is_symlink() {
                continue;
            }

            let metadata = match fs::metadata(path) {
                Ok(m) => m,
                Err(_) => continue,
            };

            entries.push(ScanEntry {
                path: path.to_path_buf(),
                size: metadata.len(),
                mod_time: metadata.modified().unwrap_or_else(|_| std::time::SystemTime::now()).into(),
                is_app_dir: false,
                app_dir_path: PathBuf::new(),
                app_dir_sig: appdir::AppDirSignature::default(),
            });
        }

        Ok(entries)
    }

    async fn hash_files(&self, entries: Vec<ScanEntry>, base_dir: &Path, progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>) -> Vec<FileRecord> {
        let sem = Arc::new(Semaphore::new(self.workers));
        let done = Arc::new(AtomicUsize::new(0));
        let total = entries.len();
        let mut handles = Vec::new();

        for entry in entries {
            let sem = sem.clone();
            let done = done.clone();
            let base_dir = base_dir.to_path_buf();
            let progress_tx = progress_tx.clone();
            let file_name = entry.path.file_name().unwrap_or_default().to_string_lossy().to_string();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let record = process_entry(entry, &base_dir);
                let current_done = done.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(ScanProgress {
                        total,
                        done: current_done,
                        current_file: file_name,
                        stage: "hashing".into(),
                    });
                }
                (record, current_done, total)
            }));
        }

        let mut records = Vec::with_capacity(total);
        for handle in handles {
            if let Ok((Some(record), _, _)) = handle.await {
                records.push(record);
            }
        }

        records
    }
}

fn process_entry(entry: ScanEntry, base_dir: &Path) -> Option<FileRecord> {
    if entry.is_app_dir {
        let sig = &entry.app_dir_sig;
        let dir_base = entry.app_dir_path.file_name()?.to_string_lossy().to_string();

        let mut exe_names = Vec::new();
        if let Ok(child_entries) = fs::read_dir(&entry.app_dir_path) {
            for ce in child_entries.flatten() {
                let name = ce.file_name().to_string_lossy().to_string();
                if name.to_lowercase().ends_with(".exe") && ce.path().is_file() {
                    exe_names.push(name);
                }
            }
        }

        let hash = compute_dir_hash(&entry.app_dir_path.to_string_lossy(), &exe_names);
        let size = compute_dir_size(&entry.app_dir_path);
        let (ver, _) = extract_version(&dir_base);
        let main_exe_path = entry.app_dir_path.join(&sig.main_exe);

        Some(FileRecord {
            id: FileRecord::new_id(&hash, &entry.app_dir_path.to_string_lossy()),
            name: sig.app_name.clone(),
            version: ver,
            local_path: main_exe_path.to_string_lossy().to_string(),
            file_size: size,
            file_hash: hash,
            extension: ".exe".to_string(),
            status: "active".to_string(),
            scanned_at: Utc::now(),
            mod_time: entry.mod_time,
            is_app_dir: true,
            app_dir_path: entry.app_dir_path.to_string_lossy().to_string(),
            app_dir_reason: sig.reason.clone(),
            ..Default::default()
        })
    } else {
        let hash = match compute_hash(&entry.path) {
            Some(h) => h,
            None => return None,
        };

        let name = entry.path.file_name()?.to_string_lossy().to_string();
        let ext = if let Some(e) = entry.path.extension() {
            format!(".{}", e.to_string_lossy())
        } else {
            String::new()
        };
        let (ver, _) = extract_version(&name);

        Some(FileRecord {
            id: FileRecord::new_id(&hash, &entry.path.to_string_lossy()),
            name,
            version: ver,
            local_path: entry.path.to_string_lossy().to_string(),
            file_size: entry.size as i64,
            file_hash: hash,
            extension: ext,
            status: "active".to_string(),
            scanned_at: Utc::now(),
            mod_time: entry.mod_time,
            ..Default::default()
        })
    }
}

pub fn compute_hash(path: &Path) -> Option<String> {
    use sha2::Digest;
    use std::io::{BufReader, Read};

    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024]; // 64KB chunks
    loop {
        let n = reader.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex::encode(hasher.finalize()))
}
