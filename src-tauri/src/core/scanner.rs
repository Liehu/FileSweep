use crate::core::appdir::{
    self, collect_executables_in_subtree, compute_dir_hash, compute_dir_size, stats_for_file,
    SubtreeStats,
};
use crate::core::models::{FileRecord, ScanProgress};
use crate::core::version::extract_version;
use chrono::Utc;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::sync::mpsc;

pub struct Scanner {
    workers: usize,
}

impl Scanner {
    pub fn new() -> Self {
        let workers = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(1);
        Self { workers }
    }

    /// 扫描目录，返回文件记录（含 app dir 聚合记录）。
    /// 签名保持稳定（对外接口）。
    pub async fn scan(
        &self,
        dir: &str,
        recursive: bool,
        detect_app_dirs: bool,
        progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>,
    ) -> Result<Vec<FileRecord>, String> {
        let abs_dir = fs::canonicalize(dir).map_err(|e| format!("解析路径失败: {}", e))?;
        let mut records = Vec::new();

        // 阶段1：收集目录树（带进度）
        let progress_for_tree = progress_tx.clone();
        let tree = collect_dir_tree(&abs_dir, recursive, Some(&move |dir_count: usize| {
            if let Some(tx) = &progress_for_tree {
                let _ = tx.send(ScanProgress {
                    total: 0,
                    done: dir_count,
                    current_file: format!("遍历目录中... ({})", dir_count),
                    stage: "walking".into(),
                });
            }
        }));

        // 阶段2-3：识别 app root（仅当 detect_app_dirs）
        let app_roots = if detect_app_dirs {
            let stats = mark_subtree_stats(&tree);
            find_app_roots(&tree, &stats)
        } else {
            Vec::new()
        };

        // app root 子树路径集合（用于跳过内部文件）
        let app_subtrees: HashSet<PathBuf> = app_roots
            .iter()
            .flat_map(|r| tree.descendants(&r.path))
            .collect();

        // app root 聚合记录
        for root in &app_roots {
            if let Some(rec) = build_app_root_record(root) {
                records.push(rec);
            }
        }

        // 阶段4：普通文件扫描（跳过 app root 内部）
        let normal_files = collect_normal_files(&tree, &app_subtrees);
        let hashed = self.hash_file_list(normal_files, &abs_dir, progress_tx).await;
        records.extend(hashed);

        Ok(records)
    }

    /// 并发哈希普通文件列表
    async fn hash_file_list(
        &self,
        files: Vec<NormalFile>,
        _base_dir: &Path,
        progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>,
    ) -> Vec<FileRecord> {
        let sem = Arc::new(Semaphore::new(self.workers));
        let done = Arc::new(AtomicUsize::new(0));
        let total = files.len();
        let mut handles = Vec::new();

        for file in files {
            let sem = sem.clone();
            let done = done.clone();
            let progress_tx = progress_tx.clone();
            let file_name = file
                .path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let record = process_normal_file(file);
                let current_done = done.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                if let Some(tx) = &progress_tx {
                    let _ = tx.send(ScanProgress {
                        total,
                        done: current_done,
                        current_file: file_name,
                        stage: "hashing".into(),
                    });
                }
                record
            }));
        }

        let mut records = Vec::with_capacity(total);
        for handle in handles {
            if let Ok(Some(record)) = handle.await {
                records.push(record);
            }
        }
        records
    }
}

// ────────────────── 阶段1：目录树收集 ──────────────────

struct DirNode {
    children: Vec<PathBuf>, // 直接子目录路径
    files: Vec<String>,     // 直接子文件名
}

struct DirTree {
    nodes: HashMap<PathBuf, DirNode>,
    root: PathBuf,
}

impl DirTree {
    /// 所有目录路径，按深度降序（深层优先，自底向上处理）
    fn dirs_depth_desc(&self) -> Vec<PathBuf> {
        let mut dirs: Vec<(PathBuf, usize)> = self
            .nodes
            .keys()
            .map(|p| (p.clone(), p.components().count()))
            .collect();
        dirs.sort_by(|a, b| b.1.cmp(&a.1));
        dirs.into_iter().map(|(p, _)| p).collect()
    }

    /// 获取某目录的所有后代目录（含自身）路径
    fn descendants(&self, dir: &Path) -> Vec<PathBuf> {
        let mut result = vec![dir.to_path_buf()];
        let mut stack: Vec<PathBuf> = self
            .nodes
            .get(dir)
            .map(|n| n.children.clone())
            .unwrap_or_default();
        while let Some(d) = stack.pop() {
            result.push(d.clone());
            if let Some(node) = self.nodes.get(&d) {
                stack.extend(node.children.iter().cloned());
            }
        }
        result
    }
}

/// 阶段1：收集目录树（单次 walkdir 遍历，同时收集目录和文件）
/// progress_fn: 可选进度回调（每处理 N 个目录调用一次）
fn collect_dir_tree(
    root: &Path,
    recursive: bool,
    progress_fn: Option<&dyn Fn(usize)>,
) -> DirTree {
    let mut nodes: HashMap<PathBuf, DirNode> = HashMap::new();
    let mut dir_count: usize = 0;

    // 确保每个目录节点存在（懒初始化）
    fn ensure_node<'a>(
        nodes: &'a mut HashMap<PathBuf, DirNode>,
        path: &Path,
    ) -> &'a mut DirNode {
        nodes.entry(path.to_path_buf()).or_insert(DirNode {
            children: vec![],
            files: vec![],
        })
    }

    let walker = if recursive {
        walkdir::WalkDir::new(root).follow_links(false).into_iter()
    } else {
        walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(1)
            .into_iter()
    };

    for entry in walker {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();

        // 跳过隐藏条目（非根目录）
        if name.starts_with('.') && path != root {
            // walkdir 仍会递归隐藏目录，但我们不记录其内容
            // 简单处理：跳过隐藏目录的直接记录，其子项也会被跳过（因为名字检查）
            continue;
        }

        let ft = entry.file_type();
        if ft.is_dir() {
            // 确保目录节点存在
            ensure_node(&mut nodes, path);
            // 建立 parent → child 关系
            if path != root {
                if let Some(parent) = path.parent() {
                    let parent_node = ensure_node(&mut nodes, parent);
                    if !parent_node.children.contains(&path.to_path_buf()) {
                        parent_node.children.push(path.to_path_buf());
                    }
                }
            }
            dir_count += 1;
            if let Some(cb) = progress_fn {
                if dir_count % 200 == 0 {
                    cb(dir_count);
                }
            }
        } else if ft.is_file() {
            // 文件加入父目录的 files 列表
            if let Some(parent) = path.parent() {
                let parent_node = ensure_node(&mut nodes, parent);
                parent_node.files.push(name);
            }
        }
        // 符号链接跳过（不跟）
    }

    // 确保 root 节点存在
    ensure_node(&mut nodes, root);

    DirTree {
        nodes,
        root: root.to_path_buf(),
    }
}


/// 读取目录的直接子文件名（跳过隐藏、跳过子目录）
fn direct_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            if e.file_type().map(|t| t.is_file()).unwrap_or(false) {
                files.push(name);
            }
        }
    }
    files
}

// ────────────────── 阶段2：子树统计标记 ──────────────────

/// 阶段2：自底向上计算每个目录的子树统计
fn mark_subtree_stats(tree: &DirTree) -> HashMap<PathBuf, SubtreeStats> {
    let mut cache: HashMap<PathBuf, SubtreeStats> = HashMap::new();

    for dir in tree.dirs_depth_desc() {
        let mut stats = SubtreeStats::default();
        if let Some(node) = tree.nodes.get(&dir) {
            for fname in &node.files {
                let (is_exec, is_py, is_aux) = stats_for_file(fname);
                if is_exec {
                    stats.has_exec = true;
                }
                if is_py {
                    stats.py_count += 1;
                }
                if is_aux {
                    stats.aux_count += 1;
                }
                stats.total_files += 1;
            }
            for child in &node.children {
                if let Some(child_stats) = cache.get(child) {
                    stats.merge_child(child_stats);
                }
            }
        }
        cache.insert(dir, stats);
    }
    cache
}

// ────────────────── 阶段3：app root 边界扩张 ──────────────────

struct AppRoot {
    path: PathBuf,
    reason: String,
    executables: Vec<String>,
}

/// 阶段3：自底向上扩张定 app root 边界
fn find_app_roots(
    tree: &DirTree,
    stats: &HashMap<PathBuf, SubtreeStats>,
) -> Vec<AppRoot> {
    // app 候选目录按深度降序
    let candidates: Vec<PathBuf> = tree
        .dirs_depth_desc()
        .into_iter()
        .filter(|d| stats.get(d).map(|s| s.is_app_candidate()).unwrap_or(false))
        .collect();

    let mut covered: HashSet<PathBuf> = HashSet::new();
    let mut roots = Vec::new();

    for dir in &candidates {
        if covered.contains(dir) {
            continue;
        }

        // 向上扩张
        let mut current = dir.clone();
        loop {
            let parent = match current.parent() {
                Some(p) if p == tree.root || p.starts_with(&tree.root) => p,
                _ => break,
            };
            let parent_is_candidate =
                stats.get(parent).map(|s| s.is_app_candidate()).unwrap_or(false);
            if !parent_is_candidate || covered.contains(parent) {
                break;
            }
            // 多软件合并修正：父含 >1 独立 exec 子树则不合并
            let parent_node = match tree.nodes.get(parent) {
                Some(n) => n,
                None => break,
            };
            let independent_exec_children: usize = parent_node
                .children
                .iter()
                .filter(|c| stats.get(*c).map(|s| s.is_app_candidate()).unwrap_or(false))
                .count();
            if independent_exec_children > 1 {
                break;
            }
            current = parent.to_path_buf();
        }

        // 标记整个子树 covered
        for desc in tree.descendants(&current) {
            covered.insert(desc);
        }

        let executables = collect_executables_in_subtree(&current);
        let reason = if executables.iter().any(|e| e.ends_with(".jar")) {
            "jar-app".to_string()
        } else if executables.is_empty() {
            "python-project".to_string()
        } else {
            "exe-app".to_string()
        };

        roots.push(AppRoot {
            path: current,
            reason,
            executables,
        });
    }
    roots
}

/// 构建 app root 的聚合 FileRecord
fn build_app_root_record(root: &AppRoot) -> Option<FileRecord> {
    let dir_base = root.path.file_name()?.to_string_lossy().to_string();
    let dir_str = root.path.to_string_lossy().to_string();

    let main_exe_rel = appdir::pick_main_exe(&root.executables, &dir_base);
    let main_exe_path = if main_exe_rel.is_empty() {
        root.path.clone()
    } else {
        root.path.join(&main_exe_rel)
    };

    let hash = compute_dir_hash(&dir_str, &root.executables);
    let size = compute_dir_size(&root.path);
    let (ver, _) = extract_version(&dir_base);
    let app_name = appdir::infer_app_name(&dir_base);

    let ext = if main_exe_rel.ends_with(".jar") {
        ".jar".to_string()
    } else if main_exe_rel.ends_with(".py") {
        ".py".to_string()
    } else {
        ".exe".to_string()
    };

    Some(FileRecord {
        id: FileRecord::new_id(&hash, &dir_str),
        name: app_name,
        version: ver,
        local_path: main_exe_path.to_string_lossy().to_string(),
        file_size: size,
        file_hash: hash,
        extension: ext,
        status: "active".to_string(),
        scanned_at: Utc::now(),
        mod_time: chrono::DateTime::from(std::time::SystemTime::now()),
        is_app_dir: true,
        app_dir_path: dir_str,
        app_dir_reason: root.reason.clone(),
        app_executables: root.executables.clone(),
        ..Default::default()
    })
}

// ────────────────── 阶段4：普通文件扫描 ──────────────────

struct NormalFile {
    path: PathBuf,
    size: u64,
    mod_time: std::time::SystemTime,
}

/// 收集不在任何 app root 子树内的普通文件
fn collect_normal_files(tree: &DirTree, app_subtrees: &HashSet<PathBuf>) -> Vec<NormalFile> {
    let mut result = Vec::new();
    for (dir_path, node) in &tree.nodes {
        if app_subtrees.contains(dir_path) {
            continue;
        }
        for fname in &node.files {
            let fpath = dir_path.join(fname);
            let metadata = match fs::metadata(&fpath) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            result.push(NormalFile {
                path: fpath,
                size: metadata.len(),
                mod_time: metadata
                    .modified()
                    .unwrap_or_else(|_| std::time::SystemTime::now()),
            });
        }
    }
    result
}

/// 处理普通文件 → FileRecord（哈希）
fn process_normal_file(file: NormalFile) -> Option<FileRecord> {
    let hash = compute_hash(&file.path)?;
    let name = file.path.file_name()?.to_string_lossy().to_string();
    let ext = if let Some(e) = file.path.extension() {
        format!(".{}", e.to_string_lossy())
    } else {
        String::new()
    };
    let (ver, _) = extract_version(&name);

    Some(FileRecord {
        id: FileRecord::new_id(&hash, &file.path.to_string_lossy()),
        name,
        version: ver,
        local_path: file.path.to_string_lossy().to_string(),
        file_size: file.size as i64,
        file_hash: hash,
        extension: ext,
        status: "active".to_string(),
        scanned_at: Utc::now(),
        mod_time: chrono::DateTime::from(file.mod_time),
        is_app_dir: false,
        app_dir_path: String::new(),
        app_dir_reason: String::new(),
        app_executables: vec![],
        ..Default::default()
    })
}

/// 计算文件 SHA256 哈希
pub fn compute_hash(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hex::encode(hasher.finalize()))
}
