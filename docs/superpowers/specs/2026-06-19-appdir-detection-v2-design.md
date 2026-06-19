# 绿色软件目录识别 v2 设计

**日期**：2026-06-19
**状态**：已批准
**关联文档**：`docs/archive/绿色软件识别.md`（旧 v1 思路，归档参考）

---

## 1. 背景与问题

现有绿色软件目录识别（`appdir.rs::detect_app_dir` + `scanner.rs::walk_dir`）存在三个结构性缺陷：

| 缺陷 | 现状 | 后果 |
|---|---|---|
| **单层判断** | `detect_app_dir` 只看当前目录的直接子文件 exe/dll 数量 | `App/bin/app.exe` 结构下，`App` 的其他子目录（data/lib/locale）的 .db/.dll 被当独立文件扫描 |
| **无向上聚合** | 识别到 app dir 后不向上找真正的"软件根目录" | 一个软件被拆成多个 app dir + 一堆散文件 |
| **walkdir 时序依赖** | 单次 walkdir 边遍历边累积 app_dir_paths，浅层文件可能在深层 app dir 识别前已入队 | 即使识别对了，跳过逻辑也可能漏 |

用户痛点：扫描会扫到大量 dll、db、未知后缀的私有文件，这些本应是绿色软件目录的内部组成。

---

## 2. 核心规则（已确认）

**向上扩张至无 exe 子树**：一个目录的整个子树（含所有后代）只要含任意可执行文件，它就是潜在软件根；从最深的含可执行文件目录向上，直到某一级的整个子树都不含可执行文件，该级为软件根。

### 2.1 可执行标记判定

| 后缀 | 说明 |
|---|---|
| .exe / .jar / .app / .bat / .cmd | 任意一个即判定目录子树"含可执行" |
| .py（特例） | **仅当**整个目录子树只有 .py（及相关辅助文件），无任何 exe/jar/app/bat/cmd 时，判定为 Python 工具项目 |

辅助文件白名单（Python 项目判定用）：.py / .pyw / .txt / .md / .cfg / .toml / .json / .rst / .ini / .yaml / .yml

### 2.2 Python 项目特例判定

避免含 `test.py` 的普通文档目录被误判。判定条件：
1. 整个子树**无** exe/jar/app/bat/cmd
2. 整个子树**至少含 1 个 .py**
3. .py + 白名单辅助文件占总文件数 **≥ 80%**

满足三者 → 判定为 Python 工具项目（如 sqlmap）。

---

## 3. 两阶段扫描架构（已确认）

```
阶段1: collect_dir_tree(root) → DirTree
  只遍历目录（不读文件元数据），构建目录树结构
  每个节点记录: 路径、直接子目录列表、直接子文件列表（仅文件名+大小，不哈希）

阶段2: mark_executable_subtrees(DirTree) → (Set<PathBuf>, Set<PathBuf>)
  自底向上：对每个目录，判断其子树是否含可执行文件（或 Python 项目）
  返回: executable_dirs（含可执行子树的目录）, python_project_dirs

阶段3: find_app_roots(DirTree, executable_dirs) → Vec<AppRoot>
  自底向上扩张：从最深的 executable_dir 开始，
  若父目录也在 executable_dirs 中 → 继续向上
  直到父目录不在 → 当前目录为 app root
  去重：一个 app root 覆盖其整个子树（标记 covered 集，跳过被覆盖的更深层目录）

阶段4: scan_files(DirTree, app_roots) → Vec<FileRecord>
  遍历文件：路径在任何 app root 子树内 → 跳过（聚合到 app root 记录）
  路径不在任何 app root 子树内 → 正常扫描入库（哈希）
  为每个 app root 生成聚合 FileRecord（含 exe/jar 列表）
```

### 3.1 阶段1: collect_dir_tree

```rust
struct DirNode {
    path: PathBuf,
    children: Vec<PathBuf>,      // 直接子目录
    files: Vec<(String, u64)>,   // 直接子文件 (name, size)
}

struct DirTree {
    nodes: HashMap<PathBuf, DirNode>,
    root: PathBuf,
}

fn collect_dir_tree(root: &Path, recursive: bool) -> DirTree {
    // walkdir 遍历，但只收集目录节点 + 各目录的直接子文件名+大小
    // 非 recursive 模式：只处理 root 的直接子项
}
```

### 3.2 阶段2: mark_executable_subtrees（自底向上 O(n)）

```rust
fn mark_executable_subtrees(tree: &DirTree) -> (HashSet<PathBuf>, HashSet<PathBuf>) {
    let mut exec_dirs = HashSet::new();
    let mut python_dirs = HashSet::new();
    let mut cache: HashMap<PathBuf, SubtreeStats> = HashMap::new();

    // 后序遍历（按深度降序处理节点）
    for dir in tree.nodes_depth_desc() {
        let mut stats = SubtreeStats::default();
        // 汇总直接子文件
        for (name, _size) in &tree.get(dir).files {
            if is_executable_marker(name) { stats.has_exec = true; }
            if name.ends_with(".py") { stats.py_count += 1; }
            if is_python_aux(name) { stats.aux_count += 1; }
            stats.total_files += 1;
        }
        // 汇总子目录的 cached stats
        for child in &tree.get(dir).children {
            if let Some(child_stats) = cache.get(child) {
                stats.merge(child_stats);
            }
        }
        // 判定
        if stats.has_exec {
            exec_dirs.insert(dir.clone());
        } else if stats.py_count >= 1 && stats.total_files > 0
            && (stats.py_count + stats.aux_count) * 100 / stats.total_files >= 80 {
            python_dirs.insert(dir.clone());
            exec_dirs.insert(dir.clone()); // python 项目也作为 executable dir 参与扩张
        }
        cache.insert(dir.clone(), stats);
    }
    (exec_dirs, python_dirs)
}
```

### 3.3 阶段3: find_app_roots（自底向上扩张）

```rust
struct AppRoot {
    path: PathBuf,
    reason: String,           // "exe-app" / "python-project" / "jar-app"
    executables: Vec<String>, // 子树所有 exe/jar/app/bat/cmd 相对路径
}

fn find_app_roots(tree: &DirTree, exec_dirs: &HashSet<PathBuf>, python_dirs: &HashSet<PathBuf>) -> Vec<AppRoot> {
    let mut covered: HashSet<PathBuf> = HashSet::new();
    let mut roots = Vec::new();

    // 按深度降序处理 exec_dirs
    for dir in exec_dirs_sorted_depth_desc() {
        if covered.contains(dir) { continue; }

        // 向上扩张
        let mut current = dir.clone();
        loop {
            let parent = current.parent();
            match parent {
                Some(p) if exec_dirs.contains(p) && !covered.contains(p) => {
                    current = p.to_path_buf();
                }
                _ => break,
            }
        }

        // current 是 app root，标记整个子树为 covered
        mark_subtree_covered(tree, &current, &mut covered);

        // 收集子树所有可执行文件
        let executables = collect_executables_in_subtree(tree, &current);
        let reason = if python_dirs.contains(&current) { "python-project" }
                     else if executables.iter().any(|e| e.ends_with(".jar")) { "jar-app" }
                     else { "exe-app" };

        roots.push(AppRoot { path: current, reason: reason.into(), executables });
    }
    roots
}
```

### 3.4 阶段4: scan_files

```rust
fn scan_files(tree: &DirTree, app_roots: &[AppRoot], base_dir: &Path, progress_tx) -> Vec<FileRecord> {
    // 构建 app_root 子树集合（快速判断文件是否在 app root 内）
    let app_subtrees: HashSet<PathBuf> = app_roots 的所有子目录路径;

    let mut records = Vec::new();

    // 1. app root 聚合记录
    for root in app_roots {
        records.push(build_app_root_record(root, tree));
    }

    // 2. 普通文件扫描（跳过 app root 内的）
    let normal_files: Vec<_> = tree.all_files().filter(|f| !in_any_app_root(f, &app_subtrees)).collect();
    let hashed = hash_files_concurrent(normal_files, progress_tx);
    records.extend(hashed);

    records
}
```

---

## 4. app dir 聚合记录

每个 app root 生成**一条** FileRecord：

| 字段 | 值 |
|---|---|
| `name` | 软件名（从目录名推断，去版本号，复用 `infer_app_name`） |
| `local_path` | 主 exe/jar 路径（pick_main_exe 从 executables 选） |
| `file_size` | 整个目录树大小（`compute_dir_size`） |
| `file_hash` | 基于路径 + exe 列表（`compute_dir_hash`） |
| `is_app_dir` | true |
| `app_dir_path` | app root 路径 |
| `app_dir_reason` | 触发原因（exe-app / jar-app / python-project） |
| `app_executables` | **新增**：`Vec<String>` 子树所有 exe/jar/app/bat/cmd 相对路径 |

### 4.1 FileRecord 新增字段

`models.rs::FileRecord` 加：
```rust
#[serde(default, rename = "appExecutables")]
pub app_executables: Vec<String>,
```

DB：file_records 加 `app_executables TEXT DEFAULT '[]'` 列（存 JSON 数组字符串），patches 幂等机制。

序列化/反序列化：DB 存 JSON 字符串，Rust 端在 row 映射时 `serde_json::from_str` 解析。

---

## 5. 与现有代码的关系

| 现有 | 改动 |
|---|---|
| `appdir.rs::detect_app_dir` | **保留**作为单目录启发式（判断 exe+dll 等用于 reason 细化），不再作为主判定 |
| `appdir.rs::pick_main_exe` | 保留（从 executables 列表选主 exe） |
| `appdir.rs::infer_app_name` | 保留 |
| `appdir.rs::compute_dir_hash/size` | 保留 |
| `scanner.rs::walk_dir` | **重写**为四阶段（collect + mark + find_roots + scan_files） |
| `scanner.rs::ScanEntry` | 简化（移除 is_app_dir/app_dir_sig，app root 在阶段3单独处理） |
| `scanner.rs::scan` | 签名不变（对外接口稳定），内部调四阶段 |
| `models.rs::FileRecord` | 加 `app_executables: Vec<String>` |
| `db/catalog.rs` | file_records 加 `app_executables` 列（patches 幂等），SELECT + row 映射 + batch_insert 更新 |
| `db/migrations.rs` | patches 加 `("file_records", "app_executables", "TEXT DEFAULT '[]'")` |

---

## 6. 关键场景验证

| 场景 | 预期结果 |
|---|---|
| `MyApp/bin/app.exe` + `MyApp/plugins/p.exe` + `MyApp/data/config.db` | app root = `MyApp`（plugins 子树有 exe 向上扩张）；config.db 不单独扫描 |
| `MyApp/app.exe` + `MyApp/data/*.dll` | app root = `MyApp`；data/*.dll 不单独扫描 |
| `Tools/sqlmap/**/*.py`（纯 Python） | app root = `sqlmap`，reason=python-project |
| `Docs/readme.md` + `Docs/test.py` | 不识别（.py 仅 1 个，且总文件可能少但需看占比；若 readme.md 是辅助文件则占比达标——需验证） |
| `Downloads/setup.exe`（单独 exe） | app root = `Downloads`？不——Downloads 是扫描根，根目录本身不作为 app root（除非显式） |

> 根目录特例：扫描的根目录（用户指定的扫描路径）**不**作为 app root 候选（避免把整个扫描根当 app）。只有根的子目录参与 app root 判定。

---

## 7. 验收标准

- [ ] `cargo check` 通过
- [ ] `npm run build` 通过
- [ ] DB migration 应用后 file_records 含 app_executables 列
- [ ] 扫描 `MyApp/bin/app.exe` 结构，结果只有 1 条 app root 记录（MyApp），内部 dll/db 不入库
- [ ] 扫描纯 Python 项目（sqlmap 风格），识别为 python-project
- [ ] 普通文件目录（无 exe）正常扫描，不误判
- [ ] app root 记录的 app_executables 含子树所有 exe/jar 路径
- [ ] 扫描进度实时更新（与 P2 forward_events 桥接兼容）

---

## 8. 风险与缓解

| 风险 | 缓解 |
|---|---|
| 阶段1 collect_dir_tree 对超大目录树内存占用 | DirTree 只存路径+文件名+大小（不存内容），典型软件库万级目录可控；若超大规模可流式处理 |
| Python 项目 80% 阈值误判 | 阈值可配置（P2 硬编码 80%，后续加 config）；辅助文件白名单收敛 |
| 向上扩张误合并无关软件 | 若两个独立软件在同一父目录（如 `Tools/AppA/app.exe` + `Tools/AppB/app.exe`），Tools 会被判为 app root 合并两者——需验证是否合理。缓解：扩张时若父目录有**多个独立含 exe 子树**，不合并（父含 exe 子目录数 > 1 时不向上） |
| 根目录误判 | 明确排除扫描根作为 app root 候选 |
| app_executables JSON 存储查询 | TEXT 列存 JSON，读取时解析；若需按 exe 查询后续可加虚拟表 |

### 关键风险细化：多软件同目录的合并问题

场景：`Tools/AppA/app.exe` + `Tools/AppB/app.exe`。
- AppA、AppB 各自是最深 exec_dir
- 向上扩张时，AppA 的父 `Tools` 也在 exec_dirs（因 Tools 子树含 exe）
- 若按原规则，Tools 会被判为 app root，合并 AppA+AppB 为一个软件——**错误**

**修正规则**：扩张时，若当前目录的父含**多于 1 个独立 exec 子树**，停止向上（父不作为 app root，当前为 root）。

判定"独立 exec 子树数"：父目录的直接子目录中，有多少个的子树含可执行文件。>1 则不合并。

---

## 9. 任务顺序

1. **DB schema**：file_records 加 app_executables 列 + FileRecord 字段
2. **appdir.rs 扩展**：加 `is_executable_marker`、Python 项目判定、保留现有启发式
3. **scanner.rs 重写**：DirTree + 四阶段（collect/mark/find_roots/scan_files）
4. **catalog.rs**：SELECT/INSERT/row 映射适配 app_executables
5. **验证**：cargo check + npm run build + 手动扫描测试目录
