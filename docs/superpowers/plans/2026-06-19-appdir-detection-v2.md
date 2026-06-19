# 绿色软件目录识别 v2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 重写扫描器的绿色软件目录识别逻辑为两阶段架构（自底向上扩张至无 exe 子树），让软件目录作为整体识别，内部 dll/db 等不再单独扫描。

**Architecture:** scanner.rs 从单次 walkdir 边遍历边识别，重写为四阶段：collect_dir_tree（构建目录树）→ mark_executable_subtrees（自底向上标记含可执行子树）→ find_app_roots（向上扩张定边界，含多软件合并修正）→ scan_files（跳过 app root 内部，聚合记录）。appdir.rs 加可执行标记判定 + Python 项目特例。

**Tech Stack:** Rust + walkdir + std::fs + rusqlite + serde

**关联设计文档：** `docs/superpowers/specs/2026-06-19-appdir-detection-v2-design.md`

**关键事实（已核实）：**
- `appdir.rs` 现有：`detect_app_dir`（单目录启发式，保留作辅助）、`pick_main_exe`、`infer_app_name`、`compute_dir_hash`、`compute_dir_size`、`is_noise_exe`、`is_doc_file`
- `scanner.rs` 现有：`Scanner::scan`（对外接口，签名 `scan(dir, recursive, detect_app_dirs, progress_tx) -> Result<Vec<FileRecord>>`，**保持不变**）、`walk_dir`（重写）、`hash_files`（保留，改为接受普通文件列表）、`process_entry`（拆分）、`compute_hash`
- `models.rs::FileRecord` 已有 `is_app_dir/app_dir_path/app_dir_reason` 字段，需加 `app_executables: Vec<String>`
- `catalog.rs` SELECT/INSERT/row 映射需适配 app_executables（JSON 字符串列）
- `migrations.rs` patches 幂等机制（`column_exists` 检查）
- 可执行后缀：.exe / .jar / .app / .bat / .cmd；Python 项目特例（纯 .py + 白名单辅助 ≥80%）
- 扫描根参与 app root 判定（无父可扩张时自然成为 root）
- 多软件合并修正：父含 >1 独立 exec 子树时不向上合并
- 文件编码：用 Edit 工具改 Rust 文件（UTF-8 安全），禁用 powershell Set-Content
- cargo PATH：`D:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin` + `D:\env\rust\cargo\bin`

---

## 文件结构

**修改：**
- `src-tauri/src/db/migrations.rs` — patches 加 app_executables 列
- `src-tauri/src/core/models.rs` — FileRecord 加 app_executables 字段
- `src-tauri/src/db/catalog.rs` — SELECT/INSERT/row 映射适配 app_executables
- `src-tauri/src/core/appdir.rs` — 加 is_executable_marker / Python 项目判定 / collect_executables_in_subtree
- `src-tauri/src/core/scanner.rs` — 重写为四阶段（DirTree + mark + find_roots + scan_files）

**保留不动：** `appdir.rs` 的 detect_app_dir/pick_main_exe/infer_app_name/compute_dir_hash/compute_dir_size/is_noise_exe/is_doc_file（作为辅助函数复用）

---

## 阶段划分

- **阶段 A（数据层）** Task 1-2：DB + FileRecord 加 app_executables。检查点 cargo check。
- **阶段 B（识别逻辑）** Task 3-4：appdir.rs 加可执行判定 + Python 项目。检查点 cargo check。
- **阶段 C（扫描器重写）** Task 5-7：DirTree + 四阶段。检查点 cargo check + 手动测试。

---

# 阶段 A：数据层

### Task 1: DB schema 加 app_executables 列 + FileRecord 字段

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/core/models.rs`

- [ ] **Step 1: patches 加 app_executables 列**

Read `src-tauri/src/db/migrations.rs`，在 patches 数组（action/move_target 之后）追加：

```rust
        ("file_records", "app_executables", "TEXT DEFAULT '[]'"),
```

完整 patches 末尾应为：
```rust
        ("file_records", "action", "TEXT DEFAULT ''"),
        ("file_records", "move_target", "TEXT DEFAULT ''"),
        ("file_records", "app_executables", "TEXT DEFAULT '[]'"),
    ];
```

- [ ] **Step 2: FileRecord 加 app_executables 字段**

Read `src-tauri/src/core/models.rs`，在 FileRecord 的 `move_target` 字段后追加：

```rust
    #[serde(default, rename = "appExecutables")]
    pub app_executables: Vec<String>,
```

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/core/models.rs
git commit -m "feat(db): file_records 加 app_executables 列 + FileRecord 字段"
```

---

### Task 2: catalog.rs SELECT/INSERT/row 映射适配 app_executables

**Files:**
- Modify: `src-tauri/src/db/catalog.rs`

- [ ] **Step 1: SELECT 加 app_executables 列**

Read `src-tauri/src/db/catalog.rs`，找到 `get_file_records` 的 SELECT 语句（约 L164），在 `action, move_target` 后加 `app_executables`：

```rust
            "SELECT id, name, version, category, local_path, file_size, file_hash,
                    extension, functional_category, status, ai_skip, scanned_at,
                    mod_time, catalog_id, is_app_dir, app_dir_path, app_dir_reason,
                    action, move_target, app_executables
             FROM file_records {}
```

- [ ] **Step 2: row 映射加 app_executables 解析**

在同一函数的 row 映射（约 L180-200），`move_target` 之后加：

```rust
                move_target: row.get::<_, String>(18).unwrap_or_default(),
                app_executables: row.get::<_, String>(19)
                    .ok()
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default(),
```

> 索引 19 是 app_executables。DB 存 JSON 字符串，读取时 serde_json::from_str 解析为 Vec<String>，失败则空数组。

- [ ] **Step 3: batch_insert_file_records 加 app_executables 列**

找到 `batch_insert_file_records`（约 L85），INSERT 语句的列列表加 `app_executables`，VALUES 加占位符。

原（17 列）：
```rust
"INSERT OR REPLACE INTO file_records
 (id, name, version, category, local_path, file_size, file_hash,
  extension, functional_category, status, ai_skip, scanned_at,
  mod_time, catalog_id, is_app_dir, app_dir_path, app_dir_reason)
 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)"
```

改为（18 列，加 app_executables）：
```rust
"INSERT OR REPLACE INTO file_records
 (id, name, version, category, local_path, file_size, file_hash,
  extension, functional_category, status, ai_skip, scanned_at,
  mod_time, catalog_id, is_app_dir, app_dir_path, app_dir_reason, app_executables)
 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)"
```

并在 execute 的 params![] 末尾加（约 L97-115 的 params 列表）：
```rust
                    serde_json::to_string(&r.app_executables).unwrap_or_else(|_| "[]".to_string()),
```

> 注意现有 params 可能用到 ?1..?17 的位置绑定，需确认是 params![] 宏展开还是显式索引。Read 实际代码确认 params 写法后调整。

- [ ] **Step 4: cargo check 验证**

```bash
cd src-tauri && set "PATH=D:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;D:\env\rust\cargo\bin;%PATH%" && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error（FileRecord 新字段在所有构造处需补默认值，可能有编译错误指向未更新的构造点——逐一修复，用 `..Default::default()` 或显式 `app_executables: vec![]`）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/db/catalog.rs src-tauri/src/core/scanner.rs src-tauri/src/commands/*.rs
git commit -m "feat(db): catalog.rs 适配 app_executables（SELECT/INSERT/row 映射）"
```

> commit 包含 scanner.rs/commands/*.rs 是因为 FileRecord 构造处（process_entry 等）需补 app_executables 字段。

---

### 📋 阶段 A 检查点

- [ ] **cargo check 通过**

---

# 阶段 B：识别逻辑（appdir.rs 扩展）

### Task 3: appdir.rs 加可执行标记判定 + Python 项目判定

**Files:**
- Modify: `src-tauri/src/core/appdir.rs`

- [ ] **Step 1: 添加可执行后缀判定函数**

在 appdir.rs 末尾（`compute_dir_size` 之后）添加：

```rust
/// 可执行文件后缀（触发 app dir 判定的标记）
pub fn is_executable_marker(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    const EXTS: &[&str] = &[".exe", ".jar", ".app", ".bat", ".cmd"];
    EXTS.iter().any(|e| lower.ends_with(e))
}

/// Python 项目辅助文件白名单（用于 Python 项目占比判定）
fn is_python_aux_file(file_name: &str) -> bool {
    let lower = file_name.to_lowercase();
    const EXTS: &[&str] = &[
        ".py", ".pyw", ".txt", ".md", ".cfg", ".toml",
        ".json", ".rst", ".ini", ".yaml", ".yml",
    ];
    EXTS.iter().any(|e| lower.ends_with(e))
}

/// 判断文件名是否为 .py
fn is_python_file(file_name: &str) -> bool {
    file_name.to_lowercase().ends_with(".py")
}
```

- [ ] **Step 2: 添加子树统计结构 + Python 项目判定**

在 appdir.rs 添加（供 scanner.rs 阶段2 使用）：

```rust
/// 目录子树的文件统计（用于自底向上标记）
#[derive(Debug, Clone, Default)]
pub struct SubtreeStats {
    pub has_exec: bool,      // 子树含可执行标记文件
    pub py_count: usize,     // .py 文件数
    pub aux_count: usize,    // Python 辅助文件数（含 .py）
    pub total_files: usize,  // 总文件数
}

impl SubtreeStats {
    /// 合并子目录的统计到当前
    pub fn merge_child(&mut self, child: &SubtreeStats) {
        self.has_exec = self.has_exec || child.has_exec;
        self.py_count += child.py_count;
        self.aux_count += child.aux_count;
        self.total_files += child.total_files;
    }

    /// 判定是否为 Python 工具项目子树（无 exe + 有 .py + 占比 ≥80%）
    pub fn is_python_project(&self) -> bool {
        if self.has_exec || self.py_count == 0 || self.total_files == 0 {
            return false;
        }
        // aux_count 已含 py_count（.py 既是 py 也是 aux）
        (self.aux_count * 100) / self.total_files >= 80
    }

    /// 子树是否应作为 app dir 候选（含可执行 或 Python 项目）
    pub fn is_app_candidate(&self) -> bool {
        self.has_exec || self.is_python_project()
    }
}

/// 从文件名更新单个文件的统计贡献
pub fn stats_for_file(file_name: &str) -> (bool, bool, bool) {
    // 返回 (is_exec, is_py, is_aux)
    let is_exec = is_executable_marker(file_name);
    let is_py = is_python_file(file_name);
    let is_aux = is_python_aux_file(file_name);
    (is_exec, is_py, is_aux)
}
```

- [ ] **Step 3: 添加 collect_executables_in_subtree 函数**

在 appdir.rs 添加（供 scanner.rs 阶段3 收集 app root 的 exe 列表）：

```rust
/// 收集目录子树下所有可执行文件的相对路径（相对于 base）
pub fn collect_executables_in_subtree(base: &Path) -> Vec<String> {
    let mut result = Vec::new();
    for entry in walkdir::WalkDir::new(base).into_iter().flatten() {
        if !entry.file_type().is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if is_executable_marker(&name) {
            // 存相对路径
            if let Ok(rel) = entry.path().strip_prefix(base) {
                result.push(rel.to_string_lossy().replace('\\', "/"));
            } else {
                result.push(name);
            }
        }
    }
    // 也收集 .py 主入口（Python 项目时，main.py 等）
    result
}
```

- [ ] **Step 4: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error（新函数未被调用也无妨）。

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/core/appdir.rs
git commit -m "feat(appdir): 加可执行标记判定 + SubtreeStats + Python 项目判定"
```

---

### 📋 阶段 B 检查点

- [ ] **cargo check 通过**

---

# 阶段 C：扫描器重写

### Task 4: scanner.rs 加 DirTree 结构与 collect_dir_tree

**Files:**
- Modify: `src-tauri/src/core/scanner.rs`

- [ ] **Step 1: 读取现有 scanner.rs 全文确认结构**

Read `src-tauri/src/core/scanner.rs` 全文。记录：
- `Scanner::scan` 签名（保持不变）
- `walk_dir` 返回 `Vec<ScanEntry>`
- `hash_files` 接受 `Vec<ScanEntry>`
- `process_entry` 处理 app dir 与普通文件
- `compute_hash`

- [ ] **Step 2: 在 scanner.rs 顶部添加 DirTree 结构**

在 scanner.rs 的 `use` 语句之后、`pub struct Scanner` 之前添加：

```rust
use crate::core::appdir::{
    self, collect_executables_in_subtree, compute_dir_hash, compute_dir_size,
    stats_for_file, SubtreeStats,
};
use std::collections::{HashMap, HashSet};

/// 目录树节点（阶段1 收集）
struct DirNode {
    path: PathBuf,
    children: Vec<PathBuf>,        // 直接子目录路径
    files: Vec<String>,            // 直接子文件名
}

/// 目录树（阶段1 产物）
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
            .map(|p| {
                let depth = p.components().count();
                (p.clone(), depth)
            })
            .collect();
        dirs.sort_by(|a, b| b.1.cmp(&a.1)); // 深度降序
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

/// 阶段1：收集目录树（只遍历结构，不读文件内容/哈希）
fn collect_dir_tree(root: &Path, recursive: bool) -> DirTree {
    let mut nodes = HashMap::new();

    // root 节点
    let root_files: Vec<String> = direct_files(root);
    nodes.insert(
        root.to_path_buf(),
        DirNode {
            path: root.to_path_buf(),
            children: vec![],
            files: root_files,
        },
    );

    if !recursive {
        // 非递归：只收集 root 的直接子目录（浅层）
        if let Ok(entries) = std::fs::read_dir(root) {
            for e in entries.flatten() {
                if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let p = e.path();
                    let files = direct_files(&p);
                    nodes.insert(
                        p.clone(),
                        DirNode {
                            path: p,
                            children: vec![],
                            files,
                        },
                    );
                }
            }
        }
        // 更新 root 的 children
        let children: Vec<PathBuf> = nodes.keys().filter(|k| **k != root).cloned().collect();
        if let Some(root_node) = nodes.get_mut(root) {
            root_node.children = children;
        }
        return DirTree {
            nodes,
            root: root.to_path_buf(),
        };
    }

    // 递归：walkdir 遍历所有子目录
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path().to_path_buf();
        if path == root {
            continue; // root 已处理
        }
        // 跳过隐藏目录
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        let files = direct_files(&path);
        nodes.insert(
            path.clone(),
            DirNode {
                path: path.clone(),
                children: vec![],
                files,
            },
        );
    }

    // 构建 children 关系：每个目录的父加入 children
    let all_dirs: Vec<PathBuf> = nodes.keys().cloned().collect();
    for dir in &all_dirs {
        if let Some(parent) = dir.parent() {
            if let Some(parent_node) = nodes.get_mut(parent) {
                if !parent_node.children.contains(dir) {
                    parent_node.children.push(dir.clone());
                }
            }
        }
    }

    DirTree {
        nodes,
        root: root.to_path_buf(),
    }
}

/// 读取目录的直接子文件名（跳过隐藏、跳过子目录）
fn direct_files(dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
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
```

> 注意 import 冲突：scanner.rs 已有 `use std::path::{Path, PathBuf};` 和 `use std::sync::Arc;` 等。新 import 需合并到现有 use 区，避免重复。`HashMap/HashSet` 需新增 import。Read 现有 use 区后合并。

- [ ] **Step 3: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error（DirTree 未被调用也无妨；可能有未使用警告，可接受）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/core/scanner.rs
git commit -m "feat(scanner): 阶段1 DirTree 结构 + collect_dir_tree"
```

---

### Task 5: scanner.rs 阶段2 mark_executable_subtrees + 阶段3 find_app_roots

**Files:**
- Modify: `src-tauri/src/core/scanner.rs`

- [ ] **Step 1: 添加阶段2 mark_executable_subtrees**

在 scanner.rs（DirTree 之后）添加：

```rust
/// app root 候选结果
struct AppRoot {
    path: PathBuf,
    reason: String,
    executables: Vec<String>,
}

/// 阶段2：自底向上标记每个目录的子树统计，返回 app 候选目录及其统计
fn mark_subtree_stats(tree: &DirTree) -> HashMap<PathBuf, SubtreeStats> {
    let mut stats_cache: HashMap<PathBuf, SubtreeStats> = HashMap::new();

    // 按深度降序（自底向上）
    for dir in tree.dirs_depth_desc() {
        let mut stats = SubtreeStats::default();
        // 直接子文件贡献
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
        }
        // 子目录贡献（已在 cache 中）
        if let Some(node) = tree.nodes.get(&dir) {
            for child in &node.children {
                if let Some(child_stats) = stats_cache.get(child) {
                    stats.merge_child(child_stats);
                }
            }
        }
        stats_cache.insert(dir, stats);
    }
    stats_cache
}

/// 阶段3：自底向上扩张定 app root 边界
fn find_app_roots(
    tree: &DirTree,
    stats: &HashMap<PathBuf, SubtreeStats>,
) -> Vec<AppRoot> {
    // app 候选目录（is_app_candidate）按深度降序
    let candidates: Vec<PathBuf> = tree
        .dirs_depth_desc()
        .into_iter()
        .filter(|d| {
            stats
                .get(d)
                .map(|s| s.is_app_candidate())
                .unwrap_or(false)
        })
        .collect();

    let mut covered: HashSet<PathBuf> = HashSet::new();
    let mut roots = Vec::new();

    for dir in &candidates {
        if covered.contains(dir) {
            continue;
        }

        // 向上扩张：父也是候选 且 父未被覆盖 且 父只有 1 个独立 exec 子树（避免多软件合并）
        let mut current = dir.clone();
        loop {
            let parent = match current.parent() {
                Some(p) if p.starts_with(&tree.root) || p == tree.root => p,
                _ => break,
            };
            // 父必须是 app 候选
            let parent_is_candidate = stats
                .get(parent)
                .map(|s| s.is_app_candidate())
                .unwrap_or(false);
            if !parent_is_candidate || covered.contains(parent) {
                break;
            }
            // 多软件合并修正：父若含 >1 独立 exec 子树则不合并
            // 独立 exec 子树 = 父的直接子目录中，子树是 app 候选的数量
            let parent_node = match tree.nodes.get(parent) {
                Some(n) => n,
                None => break,
            };
            let independent_exec_children: usize = parent_node
                .children
                .iter()
                .filter(|c| {
                    stats
                        .get(*c)
                        .map(|s| s.is_app_candidate())
                        .unwrap_or(false)
                })
                .count();
            if independent_exec_children > 1 {
                break; // 父含多个独立软件，不合并
            }
            current = parent.to_path_buf();
        }

        // current 是 app root，标记整个子树 covered
        for desc in tree.descendants(&current) {
            covered.insert(desc);
        }

        // 收集可执行文件
        let executables = collect_executables_in_subtree(&current);
        let reason = if executables.iter().any(|e| e.ends_with(".jar")) {
            "jar-app".to_string()
        } else if executables.is_empty() {
            // 无 exe/jar，可能是 Python 项目
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
```

- [ ] **Step 2: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error。

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/core/scanner.rs
git commit -m "feat(scanner): 阶段2 mark_subtree_stats + 阶段3 find_app_roots"
```

---

### Task 6: scanner.rs 阶段4 scan_files + 重写 scan/walk_dir

**Files:**
- Modify: `src-tauri/src/core/scanner.rs`

- [ ] **Step 1: 添加阶段4 scan_files + build_app_root_record**

在 scanner.rs 添加：

```rust
/// 构建 app root 的聚合 FileRecord
fn build_app_root_record(root: &AppRoot) -> Option<FileRecord> {
    let dir_base = root.path.file_name()?.to_string_lossy().to_string();
    let dir_str = root.path.to_string_lossy().to_string();

    // 选主 exe（从 executables 列表）
    let main_exe_rel = appdir::pick_main_exe_from_list(&root.executables, &dir_base);
    let main_exe_path = if main_exe_rel.is_empty() {
        root.path.clone()
    } else {
        root.path.join(&main_exe_rel)
    };

    let hash = compute_dir_hash(&dir_str, &root.executables);
    let size = compute_dir_size(&root.path);
    let (ver, _) = crate::core::version::extract_version(&dir_base);
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
        scanned_at: chrono::Utc::now(),
        mod_time: std::time::SystemTime::now().into(),
        is_app_dir: true,
        app_dir_path: dir_str,
        app_dir_reason: root.reason.clone(),
        app_executables: root.executables.clone(),
        ..Default::default()
    })
}
```

> 注意：`pick_main_exe_from_list` 是新函数，需在 appdir.rs 添加（见 Step 2）。或复用现有 `pick_main_exe`（它接受 `&[String]` + dir_name，正好匹配）。检查 appdir.rs 的 pick_main_exe 签名——它接受 `candidates: &[String], dir_name: &str`。这里 executables 是相对路径如 "bin/app.exe"，pick_main_exe 对文件名做 levenshtein，应该可用。改为直接调用 `appdir::pick_main_exe(&root.executables, &dir_base)`。

修正 build_app_root_record 的 main_exe 选择：
```rust
let main_exe_rel = appdir::pick_main_exe(&root.executables, &dir_base);
```

- [ ] **Step 2: 重写 Scanner::scan 调用四阶段**

替换 `Scanner::scan` 方法体（保持签名不变）：

```rust
pub async fn scan(
    &self,
    dir: &str,
    recursive: bool,
    detect_app_dirs: bool,
    progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>,
) -> Result<Vec<FileRecord>, String> {
    let abs_dir = fs::canonicalize(dir).map_err(|e| format!("解析路径失败: {}", e))?;

    let mut records = Vec::new();

    if detect_app_dirs {
        // 四阶段：app dir 识别
        let tree = collect_dir_tree(&abs_dir, recursive);
        let stats = mark_subtree_stats(&tree);
        let app_roots = find_app_roots(&tree, &stats);

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

        // 普通文件扫描（跳过 app root 内部）
        let normal_files = collect_normal_files(&tree, &app_subtrees, &abs_dir);
        let hashed = self.hash_file_list(normal_files, &abs_dir, progress_tx).await;
        records.extend(hashed);
    } else {
        // 不识别 app dir：传统全扫描
        let tree = collect_dir_tree(&abs_dir, recursive);
        let app_subtrees: HashSet<PathBuf> = HashSet::new();
        let normal_files = collect_normal_files(&tree, &app_subtrees, &abs_dir);
        let hashed = self.hash_file_list(normal_files, &abs_dir, progress_tx).await;
        records.extend(hashed);
    }

    Ok(records)
}
```

- [ ] **Step 3: 添加 collect_normal_files + hash_file_list**

在 scanner.rs 添加：

```rust
/// 收集不在任何 app root 子树内的普通文件（含路径、大小、修改时间）
struct NormalFile {
    path: PathBuf,
    size: u64,
    mod_time: std::time::SystemTime,
}

fn collect_normal_files(
    tree: &DirTree,
    app_subtrees: &HashSet<PathBuf>,
    _base: &Path,
) -> Vec<NormalFile> {
    let mut result = Vec::new();
    for (dir_path, node) in &tree.nodes {
        // 该目录若在 app subtree 内，跳过其文件
        if app_subtrees.contains(dir_path) {
            continue;
        }
        for fname in &node.files {
            let fpath = dir_path.join(fname);
            // 再次确认文件所在目录不在 app subtree（防御性）
            if app_subtrees.contains(dir_path) {
                continue;
            }
            let metadata = match std::fs::metadata(&fpath) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            result.push(NormalFile {
                path: fpath,
                size: metadata.len(),
                mod_time: metadata.modified().unwrap_or_else(|_| std::time::SystemTime::now()),
            });
        }
    }
    result
}
```

重写 `hash_files` 为 `hash_file_list`（接受 NormalFile 列表）：

```rust
async fn hash_file_list(
    &self,
    files: Vec<NormalFile>,
    base_dir: &Path,
    progress_tx: Option<mpsc::UnboundedSender<ScanProgress>>,
) -> Vec<FileRecord> {
    let sem = Arc::new(Semaphore::new(self.workers));
    let done = Arc::new(AtomicUsize::new(0));
    let total = files.len();
    let mut handles = Vec::new();

    for file in files {
        let sem = sem.clone();
        let done = done.clone();
        let base_dir = base_dir.to_path_buf();
        let progress_tx = progress_tx.clone();
        let file_name = file
            .path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.unwrap();
            let record = process_normal_file(file, &base_dir);
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
```

- [ ] **Step 4: 添加 process_normal_file（替代旧 process_entry 的普通文件分支）**

```rust
fn process_normal_file(file: NormalFile, _base_dir: &Path) -> Option<FileRecord> {
    let hash = compute_hash(&file.path)?;
    let name = file.path.file_name()?.to_string_lossy().to_string();
    let ext = if let Some(e) = file.path.extension() {
        format!(".{}", e.to_string_lossy())
    } else {
        String::new()
    };
    let (ver, _) = crate::core::version::extract_version(&name);

    Some(FileRecord {
        id: FileRecord::new_id(&hash, &file.path.to_string_lossy()),
        name,
        version: ver,
        local_path: file.path.to_string_lossy().to_string(),
        file_size: file.size as i64,
        file_hash: hash,
        extension: ext,
        status: "active".to_string(),
        scanned_at: chrono::Utc::now(),
        mod_time: file.mod_time.into(),
        is_app_dir: false,
        app_dir_path: String::new(),
        app_dir_reason: String::new(),
        app_executables: vec![],
        ..Default::default()
    })
}
```

- [ ] **Step 5: 删除旧的 walk_dir / hash_files / process_entry / ScanEntry**

删除：
- `struct ScanEntry` 定义
- `fn walk_dir`
- `async fn hash_files`（被 hash_file_list 替代）
- `fn process_entry`（被 build_app_root_record + process_normal_file 替代）

保留：`Scanner::new`、`compute_hash`。

- [ ] **Step 6: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error。若有未使用 import 警告（如 `appdir::detect_app_dir` 不再调用），保留（detect_app_dir 作为辅助保留，可能未来用）；或加 `#[allow(unused_imports)]`。

- [ ] **Step 7: 提交**

```bash
git add src-tauri/src/core/scanner.rs src-tauri/src/core/appdir.rs
git commit -m "feat(scanner): 阶段4 scan_files + 重写 scan 为四阶段架构

- collect_dir_tree → mark_subtree_stats → find_app_roots → scan_files
- app root 聚合记录（含 app_executables）
- 删除旧 walk_dir/hash_files/process_entry/ScanEntry
- appdir.pick_main_exe 复用于 executables 列表"
```

---

### 📋 阶段 C 检查点

- [ ] **cargo check 通过**

- [ ] **手动测试（用户在 tauri dev 环境）**

准备测试目录结构：
```
D:\TestScan\
  MyApp\
    bin\app.exe
    plugins\p.exe
    data\config.db
    lib\lib1.dll
  sqlmap\
    sqlmap.py
    lib\__init__.py
    data\queries.txt
  LooseFile.txt
  setup.exe
```

扫描 `D:\TestScan`，验证：
1. `MyApp` 识别为 app root（exe-app），内部 config.db/lib1.dll 不单独入库
2. `sqlmap` 识别为 app root（python-project）
3. `LooseFile.txt` 作为普通文件入库
4. `setup.exe` 作为普通文件入库（或 D:\TestScan 本身被识别为 app root？取决于 setup.exe 是否让根成为候选——根含 LooseFile.txt + setup.exe，setup.exe 是 exe 标记，根子树含 exec → 根是候选；但根有 MyApp/sqlmap 两个独立 exec 子树 >1 → 不合并，根不作为 app root；setup.exe 作为普通文件入库）
5. MyApp 记录的 appExecutables 含 ["bin/app.exe", "plugins/p.exe"]

DevTools 验证：
```js
const files = await window.__TAURI__.core.invoke("plugin_invoke", {plugin:"filesweep", action:"scan:files", args:{page:1, pageSize:100}})
// 检查结果
```

---

## 验收标准（对齐设计文档第 7 节）

- [ ] cargo check 通过
- [ ] npm run build 通过
- [ ] DB migration 应用后 file_records 含 app_executables 列
- [ ] 扫描 MyApp/bin/app.exe 结构，只有 1 条 app root（MyApp），内部 dll/db 不入库
- [ ] 扫描纯 Python 项目（sqlmap 风格），识别为 python-project
- [ ] 普通文件目录不误判
- [ ] app root 记录的 app_executables 含子树所有 exe/jar 路径
- [ ] 扫描进度实时更新

---

## 回滚指引

若四阶段架构有问题，scanner.rs 可从 git 恢复：
```bash
git checkout HEAD~<n> -- src-tauri/src/core/scanner.rs
```
DB schema 改动（app_executables 列）是增量的，不影响旧代码运行（旧 INSERT 不含该列，DB 用 DEFAULT '[]'）。

---

## 已知风险点（实现时关注）

1. **walkdir 隐藏目录跳过**：阶段1 collect_dir_tree 用 walkdir 遍历时跳过隐藏目录（`name.starts_with('.')`），与原逻辑一致。但非递归模式的浅层处理需单独处理 root 的直接子目录。

2. **app_subtrees 跳过判断的准确性**：collect_normal_files 检查 `dir_path in app_subtrees`。app_subtrees 是 app_roots 各自的 descendants（含自身）。确保 descendants 正确包含所有层级。

3. **pick_main_exe 与 executables 格式**：executables 是相对路径（如 "bin/app.exe"），pick_main_exe 原设计接受文件名（如 "app.exe"）。需确认 levenshtein 对相对路径是否有效——可能需取 basename 后比较。实现时检查 pick_main_exe 实现，必要时在 appdir.rs 加 `pick_main_exe_from_rel_paths` 变体。

4. **hash_file_list 进度**：total 是普通文件数（不含 app root）。app root 聚合不发送 progress（瞬间完成）。前端进度条 total 可能与预期略差（app root 内部文件不计数），可接受。

5. **FileRecord 构造点**：除 scanner.rs 外，其他地方构造 FileRecord 的（如 clean.rs 的 parse_frontend_actions 用 `..Default::default()`）会自动得到 `app_executables: vec![]`（Default），无需改。但显式构造的需补字段——cargo check 会指出所有遗漏点。
