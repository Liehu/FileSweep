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
        let tree = collect_dir_tree(&abs_dir, recursive, Some(&move |dir_count: usize, file_count: usize| {
            if let Some(tx) = &progress_for_tree {
                let _ = tx.send(ScanProgress::indeterminate(
                    "walking",
                    "遍历目录",
                    dir_count,
                    format!("已发现 {} 个目录，{} 个文件", dir_count, file_count),
                ));
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

    /// 并发哈希普通文件列表。
    /// 进度事件经节流（每 150ms 最多一次）并附带速率/ETA，避免大量文件时 IPC 洪泛。
    async fn hash_file_list(
        &self,
        files: Vec<NormalFile>,
        _base_dir: &Path,
        progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>,
    ) -> Vec<FileRecord> {
        use std::time::{Duration, Instant};

        let sem = Arc::new(Semaphore::new(self.workers));
        let done = Arc::new(AtomicUsize::new(0));
        let total = files.len();

        // 最近处理完的文件名（用于展示），用 latest_name 通道把名字捎带回 reporter
        let (name_tx, mut name_rx) = mpsc::unbounded_channel::<String>();

        // 启动一个 reporter 任务：周期性采样 done，计算速率/ETA，发送进度事件
        let reporter = {
            let done = done.clone();
            let progress_tx = progress_tx.clone();
            tokio::spawn(async move {
                let Some(tx) = progress_tx else { return; };
                let mut last_name = String::new();
                let mut last_report = Instant::now();
                let mut last_done: usize = 0;
                loop {
                    tokio::time::sleep(Duration::from_millis(150)).await;
                    // 拿最新文件名（取最后一条，丢弃旧的）
                    while let Ok(n) = name_rx.try_recv() {
                        last_name = n;
                    }
                    let current_done = done.load(AtomicOrdering::Relaxed);
                    let now = Instant::now();

                    // 速率：用最近一个采样窗口的瞬时速率，比全程平均更能反映当前速度
                    let window = now.duration_since(last_report).as_secs_f64();
                    let rate = if window > 0.0 {
                        (current_done.saturating_sub(last_done)) as f64 / window
                    } else {
                        0.0
                    };
                    last_done = current_done;
                    last_report = now;

                    let eta_sec = if rate > 0.0 && total > current_done {
                        ((total - current_done) as f64 / rate) as i64
                    } else {
                        0
                    };

                    let _ = tx.send(ScanProgress::determinate(
                        "hashing",
                        "计算哈希",
                        total,
                        current_done,
                        last_name.clone(),
                        rate,
                        eta_sec,
                    ));

                    if current_done >= total {
                        break;
                    }
                }
            })
        };

        let mut handles = Vec::with_capacity(total);
        for file in files {
            if crate::commands::scan::is_scan_cancelled() {
                log::info!("扫描在哈希阶段被取消，剩余文件跳过");
                break;
            }
            let sem = sem.clone();
            let done = done.clone();
            let name_tx = name_tx.clone();

            handles.push(tokio::spawn(async move {
                let _permit = sem.acquire().await.unwrap();
                let record = process_normal_file(file);
                let _ = done.fetch_add(1, AtomicOrdering::Relaxed);
                // 捎带文件名给 reporter（非关键，发送失败忽略）
                if let Some(r) = &record {
                    let _ = name_tx.send(r.name.clone());
                }
                record
            }));
        }
        // 释放发送端，reporter 的 try_recv 才能感知结束
        drop(name_tx);

        let mut records = Vec::with_capacity(total);
        for handle in handles {
            if let Ok(Some(record)) = handle.await {
                records.push(record);
            }
        }
        // 等 reporter 自然结束（它会在 done>=total 时退出）
        let _ = reporter.await;
        // 确保 100% 最终事件被发出（reporter 已发过，这里兜底）
        if let Some(tx) = &progress_tx {
            let _ = tx.send(ScanProgress::determinate(
                "hashing",
                "计算哈希",
                total,
                total,
                String::new(),
                0.0,
                0,
            ));
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

/// 阶段1：收集目录树。
/// recursive=true 时，对顶层子目录并行 walkdir 遍历（多 task），合并结果。
/// progress_fn: 可选进度回调（参数：目录数、文件数）。
fn collect_dir_tree(
    root: &Path,
    recursive: bool,
    progress_fn: Option<&dyn Fn(usize, usize)>,
) -> DirTree {
    fn ensure_node<'a>(
        nodes: &'a mut HashMap<PathBuf, DirNode>,
        path: &Path,
    ) -> &'a mut DirNode {
        nodes.entry(path.to_path_buf()).or_insert(DirNode {
            children: vec![],
            files: vec![],
        })
    }

    // 单个子树的遍历（同步，在一个 task 内运行）
    // 返回该子树的所有 nodes + 该子树根节点自身的 files
    fn walk_subtree(sub_root: &Path) -> HashMap<PathBuf, DirNode> {
        let mut nodes: HashMap<PathBuf, DirNode> = HashMap::new();
        for entry in walkdir::WalkDir::new(sub_root)
            .follow_links(false)
            .into_iter()
        {
            if crate::commands::scan::is_scan_cancelled() {
                break;
            }
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let ft = entry.file_type();
            if ft.is_dir() {
                ensure_node(&mut nodes, path);
                if path != sub_root {
                    if let Some(parent) = path.parent() {
                        let pn = ensure_node(&mut nodes, parent);
                        if !pn.children.contains(&path.to_path_buf()) {
                            pn.children.push(path.to_path_buf());
                        }
                    }
                }
            } else if ft.is_file() {
                if let Some(parent) = path.parent() {
                    ensure_node(&mut nodes, parent).files.push(name);
                }
            }
        }
        nodes
    }

    let mut nodes: HashMap<PathBuf, DirNode> = HashMap::new();

    // root 节点
    ensure_node(&mut nodes, root);

    // 读 root 的直接条目（区分文件和子目录）
    let mut top_files: Vec<String> = Vec::new();
    let mut top_dirs: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(root) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            match e.file_type() {
                Ok(ft) if ft.is_dir() => top_dirs.push(e.path()),
                Ok(ft) if ft.is_file() => top_files.push(name),
                _ => {}
            }
        }
    }

    // root 的文件
    nodes.get_mut(root).unwrap().files = top_files;

    if !recursive {
        // 非递归：只记录顶层子目录节点（不遍历内部）
        for d in &top_dirs {
            ensure_node(&mut nodes, d);
            nodes.get_mut(root).unwrap().children.push(d.clone());
        }
        if let Some(cb) = progress_fn {
            cb(top_dirs.len(), nodes.get(root).unwrap().files.len());
        }
        return DirTree {
            nodes,
            root: root.to_path_buf(),
        };
    }

    // 并行遍历各顶层子目录（每个 task 独立返回 HashMap，最后合并）
    let mut handles = Vec::new();
    for sub_dir in &top_dirs {
        let sd = sub_dir.clone();
        handles.push(std::thread::spawn(move || walk_subtree(&sd)));
    }

    // 收集各子树结果并合并
    for h in handles {
        if let Ok(sub_nodes) = h.join() {
            for (path, node) in sub_nodes {
                nodes.entry(path).or_insert(node);
            }
        }
    }

    // 重建 root 的 children（顶层子目录）
    nodes.get_mut(root).map(|n| n.children = top_dirs.clone());

    // 统计总数发进度
    if let Some(cb) = progress_fn {
        let dc = nodes.len();
        let fc: usize = nodes.values().map(|n| n.files.len()).sum();
        cb(dc, fc);
    }

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
                appdir::stats_for_file(fname, &mut stats);
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

/// 判定一个目录是否为"软件集合目录"（不应作为单个 app root，其散装文件走普通扫描）。
///
/// 判定：含 ≥2 个【独立软件子目录】。"独立软件子目录"指：
///   - 子目录是 app_candidate（子树含可执行文件）
///   - 且子目录名不是通用数据目录名（data/user/lib/logs/temp/cache/config/plugins/assets/bin 等）
///
/// 这样区分：
///   - D:\programs：shiro_attack + 红明谷 + ztasker → ≥2 独立软件子目录 → 集合 ✓
/// 目录分类结果
#[derive(Debug, Clone, PartialEq)]
pub enum DirClassification {
    AppDir(String),      // 是 app dir，reason 为 exe-app/jar-app/python-project
    Collection,          // 集合目录（跳过，内部展开）
    Unrecognized,        // 无法确定，由用户判断
}

/// 综合评分判定目录类型：app dir / 集合 / unrecognized
///
/// 三维度：文件类型占比 + 目录层级 + 子目录结构
fn classify_dir(
    dir: &Path,
    tree: &DirTree,
    stats: &HashMap<PathBuf, SubtreeStats>,
) -> DirClassification {
    let dir_stats = match stats.get(dir) {
        Some(s) => s,
        None => return DirClassification::Unrecognized,
    };
    let node = match tree.nodes.get(dir) {
        Some(n) => n,
        None => return DirClassification::Unrecognized,
    };

    // Python 项目特例
    if dir_stats.is_python_project() {
        return DirClassification::AppDir("python-project".into());
    }

    // 小目录特例：总文件 ≤50 且有 exec → 直接 app dir
    // 但仍需排除集合目录（散装 exe 集合 / 多软件子目录）
    if dir_stats.total_files <= 50 && dir_stats.has_exec() {
        // 检查是否为集合：直接 exe ≥5（散装集合）或独立软件子目录 ≥2
        let direct_exec_count = node
            .files
            .iter()
            .filter(|f| appdir::is_executable_marker(f))
            .count();
        let independent_sw_children: usize = node
            .children
            .iter()
            .filter(|c| {
                if !stats.get(*c).map(|s| s.is_app_candidate()).unwrap_or(false) {
                    return false;
                }
                let name = c
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase())
                    .unwrap_or_default();
                !is_data_dir_name(&name)
            })
            .count();
        if direct_exec_count >= 5 || independent_sw_children >= 2 {
            return DirClassification::Collection;
        }
        let reason = if dir_stats.exec_count > 0 {
            let has_jar = node.files.iter().any(|f| f.to_lowercase().ends_with(".jar"));
            if has_jar { "jar-app" } else { "exe-app" }
        } else {
            "exe-app"
        };
        return DirClassification::AppDir(reason.into());
    }

    // 统计信号
    let data_ratio = dir_stats.data_ratio();
    let exec_ratio = dir_stats.exec_ratio();
    let archive_ratio = dir_stats.archive_ratio();

    // 直接子文件的 exec 数（区分散装 exe 集合）
    let direct_exec_count = node
        .files
        .iter()
        .filter(|f| appdir::is_executable_marker(f))
        .count();

    // 独立软件子目录数
    let independent_sw_children: usize = node
        .children
        .iter()
        .filter(|c| {
            if !stats.get(*c).map(|s| s.is_app_candidate()).unwrap_or(false) {
                return false;
            }
            let name = c
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            !is_data_dir_name(&name)
        })
        .count();

    // 配套子目录数（data/lib/bin...）
    let companion_children: usize = node
        .children
        .iter()
        .filter(|c| {
            let name = c
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            is_data_dir_name(&name)
        })
        .count();

    // 评分
    let mut score: i32 = 0;
    // +2: dll/依赖主导（ztasker 的 40+ dll）
    if data_ratio > 0.4 {
        score += 2;
    }
    // +1: 有配套子目录（单软件结构）
    if companion_children > 0 {
        score += 1;
    }
    // +1: exec+dll 混合（典型软件）
    if exec_ratio > 0.02 && exec_ratio < 0.3 && data_ratio > 0.1 {
        score += 1;
    }
    // -2: 压缩包主导（归档集合）
    if archive_ratio > 0.5 {
        score -= 2;
    }
    // -2: 散装 exe 集合（Programs 的 30+ 不同 exe）
    if exec_ratio > 0.05 && direct_exec_count >= 5 {
        score -= 2;
    }
    // -3: 多个独立软件子目录（shiro 含 2 个 shiro_attack）→ 强集合信号
    if independent_sw_children >= 2 {
        score -= 3;
    }

    if score >= 1 {
        let reason = if dir_stats.exec_count > 0 { "exe-app" } else { "exe-app" };
        DirClassification::AppDir(reason.into())
    } else {
        // score ≤0 → 集合目录（含 unrecognized 归入集合，内部展开为普通文件）
        DirClassification::Collection
    }
}

/// 判定目录名是否为通用数据/依赖目录名（非独立软件）
fn is_data_dir_name(name_lower: &str) -> bool {
    const DATA_DIRS: &[&str] = &[
        "data", "user", "users", "lib", "libs", "library",
        "logs", "log", "temp", "tmp", "cache",
        "config", "configs", "conf", "settings",
        "plugins", "plugin", "extensions", "ext",
        "assets", "asset", "resources", "res", "resource",
        "bin", "binary", "sbin",
        "node_modules", "vendor", "deps", "dependencies",
        "jre", "jdk", "runtime", "rt",
        "bundle", "bundles", "modules", "module",
        "locale", "locales", "lang", "i18n",
        "doc", "docs", "help",
        "meta-inf",
    ];
    if DATA_DIRS.contains(&name_lower) {
        return true;
    }
    if name_lower.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        return true;
    }
    if name_lower.starts_with('v') && name_lower.len() > 1 {
        let rest = &name_lower[1..];
        if rest.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            return true;
        }
    }
    false
}

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
            // 父若是集合目录则不合并
            if classify_dir(parent, tree, stats) == DirClassification::Collection {
                break;
            }
            current = parent.to_path_buf();
        }

        // 当前目录分类判定
        match classify_dir(&current, tree, stats) {
            DirClassification::Collection => {
                covered.insert(current.clone());
                continue;
            }
            DirClassification::Unrecognized => {
                // 无法确定：标记 covered 跳过，内部文件走普通扫描（用户后续可手动标记）
                log::info!("[find_app_roots] unrecognized: {:?}", current);
                covered.insert(current.clone());
                continue;
            }
            DirClassification::AppDir(reason) => {
                // 标记整个子树 covered
                let descendants = tree.descendants(&current);
                for desc in &descendants {
                    covered.insert(desc.clone());
                }
                // 从 DirTree 收集可执行文件（纯内存）
                let executables: Vec<String> = descendants
                    .iter()
                    .filter_map(|d| tree.nodes.get(d))
                    .flat_map(|n| n.files.iter())
                    .filter(|f| appdir::is_executable_marker(f))
                    .cloned()
                    .collect();
                roots.push(AppRoot {
                    path: current,
                    reason,
                    executables,
                });
            }
        }
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
    // size: 暂用 0（compute_dir_size 的 walkdir 对 698 个 app root 太慢）
    // 验证阶段后改为从 DirTree 汇总文件大小
    let size: i64 = 0;
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

/// 判断文件是否值得计算全文 SHA256。
/// 可执行文件与安装包（含大型可执行文件）一律哈希；
/// 其它纯数据/日志/配置类文件跳过全文哈希以避免在大目录下耗时过长。
/// 判定文件是否为内容型文件（需 partial hash 精确去重）
fn is_content_file(ext_lower: &str) -> bool {
    const CONTENT_EXTS: &[&str] = &[
        ".doc", ".docx", ".pdf", ".ppt", ".pptx", ".xls", ".xlsx",
        ".odt", ".rtf", ".epub",
        ".jpg", ".jpeg", ".png", ".gif", ".bmp", ".tiff", ".webp", ".svg", ".ico",
        ".mp4", ".mkv", ".avi", ".mov", ".wmv", ".flv", ".webm",
        ".mp3", ".flac", ".wav", ".aac", ".ogg", ".wma",
        ".psd", ".ai", ".indd", ".sketch",
        ".sqlite", ".db", ".mdb",
    ];
    CONTENT_EXTS.contains(&ext_lower)
}

/// 处理普通文件 → FileRecord
/// exe/dll/安装包：元数据 hash（去重靠版本比对，无需全文）
/// 内容文件（doc/pdf/img/video）：partial hash（头尾 4KB + 大小）
/// 其他文件：元数据 hash
fn process_normal_file(file: NormalFile) -> Option<FileRecord> {
    let name = file.path.file_name()?.to_string_lossy().to_string();
    let ext = if let Some(e) = file.path.extension() {
        format!(".{}", e.to_string_lossy())
    } else {
        String::new()
    };
    let ext_lower = ext.to_lowercase();
    let (ver, _) = extract_version(&name);

    let hash = if is_content_file(&ext_lower) {
        compute_partial_hash(&file.path, file.size)?
    } else {
        compute_metadata_hash(&file)
    };

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

/// Partial hash：读文件头尾各 4KB + 文件大小，SHA256。
/// 极快（最多读 8KB），碰撞率低（头尾内容 + 大小兜底）。
fn compute_partial_hash(path: &Path, size: u64) -> Option<String> {
    use std::io::{Read, Seek, SeekFrom};

    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();

    // 文件大小作为 hash 一部分
    hasher.update(size.to_le_bytes());

    const CHUNK: usize = 4 * 1024; // 4KB

    if size <= (CHUNK as u64) * 2 {
        // 小文件：全文 hash
        let mut buf = Vec::with_capacity(size as usize);
        file.read_to_end(&mut buf).ok()?;
        hasher.update(&buf);
    } else {
        // 头 4KB
        let mut head = [0u8; CHUNK];
        let n = file.read(&mut head).ok()?;
        hasher.update(&head[..n]);

        // 尾 4KB
        file.seek(SeekFrom::End(-(CHUNK as i64))).ok()?;
        let mut tail = [0u8; CHUNK];
        let n = file.read(&mut tail).ok()?;
        hasher.update(&tail[..n]);
    }

    Some(hex::encode(hasher.finalize()))
}

/// 元数据 hash：路径 + 大小 + 修改时间（不读文件内容，瞬时）
fn compute_metadata_hash(file: &NormalFile) -> String {
    let mut hasher = Sha256::new();
    hasher.update(file.path.to_string_lossy().as_bytes());
    hasher.update(b"|");
    hasher.update(file.size.to_le_bytes());
    hasher.update(b"|");
    let nanos = file
        .mod_time
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    hasher.update(nanos.to_le_bytes());
    hex::encode(hasher.finalize())
}

/// 计算文件全文 SHA256 哈希（保留给需要精确全文的场景）
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
