# 🔥 Grill 产出：文件资源管理器窗口（Total Commander 式）

**日期**：2026-07-06
**需求**：在 FileSweep 上加一个类似 Total Commander 的文件资源管理器窗口；浏览某个文件夹时，上方菜单栏显示「一键整理」「AI 丰富」等动作。
**性质**：grill-me 拷问结论 + 推荐方案（待你批改）

---

## 0. TL;DR（我的强推荐）

**分三步走，先走通最便宜的一条路：**

1. **形态**：App 内新增 `/explorer` 视图（不开独立 OS 窗口）。单栏 + 面包屑 + 目录树，**不**上双栏/标签页。
2. **数据**：新增轻量后端命令 `fs:list_dir`（实时 `read_dir`，不哈希不入库）。浏览秒开。
3. **动作闭环**：「一键整理 / AI 丰富」按钮点了之后，**对当前文件夹做一次"定向扫描"** → 然后调现有的 `clean:start` / `enrich:start`，但**必须先解决"扫描=全局清库"这个硬冲突**（见 §2）。

如果你只想最快见到东西、不在意整理闭环，就只做第 1、2 步，按钮先放占位。

---

## 1. 你的想法和现有系统的 4 个硬冲突（grill 核心）

这 4 个不解决，任何方案都是空中楼阁：

### ⚠️ 冲突 1：扫描 = 全局清库
- **现状**：`scan:start` → `batch_insert_file_records`（`db/catalog.rs:85`）先 `DELETE FROM file_records` 再插。
- **后果**：在浏览器里点开 A 文件夹"分析一下"，会**抹掉之前扫的 B 文件夹的全部记录、catalog、AI 丰富结果**。
- **和 TC 习惯的对撞**：TC 用户随手点几个文件夹看，每次点都清库，体验崩溃。

### ⚠️ 冲突 2：整理/丰富是全局的，没有"按目录"
- **现状**：
  - `enrich:start`（`commands/enrich.rs:78`）拉 `get_file_records("", "active", "", 1, 1_000_000)`——**全表**。
  - `clean:start` 自动模式（`commands/clean.rs:41`）也是全表。
  - `get_file_records_filtered` 的 `dir_type` 过滤的是 `app_dir_reason`（目录类型枚举），**不是文件路径**。`search` 只匹配 `name LIKE`，**没有 `local_path` 过滤**。
- **后果**：你说"浏览 D:\Downloads 时点一键整理"，但点了之后整理的是**全库**，不是这个文件夹。直觉严重不符。

### ⚠️ 冲突 3：没有实时目录浏览命令
- **现状**：所有"文件列表"都来自 `file_records` 表（扫过的）。前端 `FileListView` 是**扫描后的扁平文件表**，没有面包屑、没有目录树、没有"打开文件夹"。`FileItem.localPath` 只在 Everything 搜索结果里当 tooltip，从不用于导航。
- **后果**：连"列一个目录的子项"都做不到，必须新增 `fs:list_dir`。

### ⚠️ 冲突 4：单窗口、无边框
- **现状**：`tauri.conf.json` 单窗口，`decorations: false`，自定义标题栏（`AppShell.vue:129-159`）。
- **后果**：你说"窗口"，但开独立 OS 窗口（`WebviewWindow`）要解决多窗口路由隔离、Pinia 状态同步、自定义标题栏复制、`data-tauri-drag-region` 重写——**复杂度翻倍且收益不明显**。App 内新视图便宜得多。

---

## 2. 关键决策（请你逐条拍板，我给了推荐）

| # | 决策 | 我的推荐 | 理由 |
|---|---|---|---|
| D1 | 独立窗口 vs App 内视图 | **App 内 `/explorer` 视图** | 复用 shell/标题栏/侧栏/命令面板，省一整层多窗口工程 |
| D2 | 双栏/标签页 vs 单栏 | **单栏 + 目录树** | 双栏是 TC 精髓但工作量×2，先验证价值再升级 |
| D3 | 数据来源 | **`fs:list_dir` 实时读** | 浏览必须秒开；扫描太重，不能每次点文件夹都哈希 |
| D4 | 「一键整理」作用范围 | **当前文件夹** | 用户直觉；需后端加 `dir_prefix` 过滤 + 定向 clean |
| D5 | 「AI 丰富」作用范围 | **当前文件夹** | 需 `enrich:start` 加 `dir_prefix` 参数 |
| D6 | 扫描策略 | **新增"定向扫描"：只替换某目录下的记录** | 解决冲突 1，不清全库 |
| D7 | 是否动现有 FileListView | **不动**，新建 `/explorer` | 风险隔离，两个视图并存 |

---

## 3. 推荐方案：分阶段实施

### 阶段 A — 浏览器骨架（最小可见，1 个会话能完成）

**目标**：能浏览任意文件夹，按钮先放占位。

**后端**：
- 新增 `core/fs_explorer.rs`：`list_dir(path) -> Vec<FsEntry>`，`read_dir` 返回 `{ name, path, is_dir, size, modified, extension }`。**不哈希、不入库、不分类**。
- 在 `plugins/filesweep/actions.rs` 注册 action `fs:list_dir`（参数 `{ path }`）。
- 可选：`fs:exists`、`fs:home_dir`（拿用户主目录作为浏览起点）。

**前端**：
- 新建 `views/ExplorerView.vue`：
  - 顶部**路径栏**（面包屑 `C:\ > Users > Spence > Downloads`，每段可点跳转）+ 上级目录按钮 `↑` + 「选择目录」按钮（复用 `@tauri-apps/plugin-dialog` 的 `open({directory:true})`）。
  - **顶部工具栏**：「一键整理」「AI 丰富」按钮（阶段 A 占位，disabled 或 toast 提示"需先分析此文件夹"）。
  - 左侧**目录树**（懒加载：点展开才 `fs:list_dir` 子目录）。
  - 右侧**文件列表**（复用 shadcn Table：名/大小/修改时间/类型；双击文件夹进入；表头排序）。
- `routes.ts` 加 `/explorer`；`nav.ts` 加导航项「文件浏览」；`index.ts` 加 feature（进命令面板）。
- 新建 `stores/explorer.ts`：`currentPath`、`entries`、`history`（前进/后退栈）、`listDir(path)`。

**交付标准**：能像资源管理器一样点开任意文件夹，秒级响应。按钮可见但未接通。

---

### 阶段 B — 打通「浏览 → 一键整理」闭环

**前置：解决冲突 1 + 冲突 2（后端改造，最硬的部分）**

- **定向扫描**：给 `batch_insert_file_records` 加 `dir_prefix` 参数，`DELETE FROM file_records WHERE local_path LIKE ?1 || '%'`（只删该目录下的记录），而非全表删。
  - 或更安全：新增 `replace_file_records_in_dir(dir, records)`，不动其它目录。
- **`scan:start` 加 `scope: "dir" | "all"`**：`scope=dir` 时走定向替换，并把 `task_id` 打成"目录扫描"任务。
- **`scan:files` 加 `dir_prefix` 过滤**：`get_file_records_filtered` 的 WHERE 里加 `AND local_path LIKE :prefix || '%'`。
- **`clean:start` 加 `dir_prefix`**：自动模式只对该目录下的记录生成 actions。

**前端**：
- ExplorerView「一键整理」按钮：
  1. 若该目录未分析 → 提示并触发 `scan:start { dirs:[currentPath], scope:"dir" }`，监听 `scan_complete`。
  2. 扫完 → `scan:suggestions_v2`（已支持 dir_type，需加 dir_prefix）。
  3. 拉一个抽屉/右侧面板显示建议（复用 `SuggestionPanel.vue` 的逻辑，但数据源切到当前目录）。
  4. 用户确认 → `clean:start { dir_prefix: currentPath, file_actions: [...] }`。

**交付标准**：在浏览器里点开一个文件夹 → 点「一键整理」→ 只分析/整理这个文件夹，不动其它库。

---

### 阶段 C — 打通「浏览 → AI 丰富」

- `enrich:start` 加 `dir_prefix` 参数：`get_file_records("", "active", "", 1, 1e6)` → 加 `WHERE local_path LIKE :prefix`。
- ExplorerView「AI 丰富」按钮：同 B 的模式，先确保已分析（有 file_records），再定向丰富，进度走现有 `enrich_progress` 事件。
- 丰富结果展示：在文件列表行里加「AI」徽标（已有 `catalog_entries` 关联），或点击文件弹侧栏显示 catalog。

---

### 阶段 D（可选，远期）— TC 高级特性

- 双栏（左右独立 currentPath）。
- 标签页。
- 快捷键（F3 查看 / F4 编辑 / F5 复制 / F6 移动 / F7 建目录 / F8 删除 / Alt+F4 退出）。
- 收藏夹 / 历史路径。
- 命令行栏。

**这些在 A-C 验证完价值后再做，不要一上来就铺。**

---

## 4. 关键代码改动点速查

| 改动 | 文件 | 说明 |
|---|---|---|
| 新建 fs 浏览后端 | `src-tauri/src/core/fs_explorer.rs`（新） | `list_dir` |
| 注册 action | `src-tauri/src/plugins/filesweep/actions.rs` | `fs:list_dir` |
| 定向扫描 | `src-tauri/src/db/catalog.rs` | `replace_file_records_in_dir` |
| scan 加 scope | `src-tauri/src/commands/scan.rs` + `actions.rs` | `scan:start { scope }` |
| dir_prefix 过滤 | `src-tauri/src/db/catalog.rs` `get_file_records_filtered` | WHERE local_path LIKE |
| enrich 加 dir | `src-tauri/src/commands/enrich.rs` | `start_enrich(dir_prefix)` |
| clean 加 dir | `src-tauri/src/commands/clean.rs` | 自动模式 dir 过滤 |
| 新视图 | `src/plugins/filesweep/views/ExplorerView.vue`（新） | 主界面 |
| 新 store | `src/plugins/filesweep/stores/explorer.ts`（新） | 浏览状态 |
| 路由/导航 | `routes.ts` / `nav.ts` / `index.ts` | `/explorer` |

---

## 5. 我对风险的判断

1. **最大工作量在阶段 B 的后端定向扫描**，不是前端浏览器。`batch_insert_file_records` 是全表替换，牵涉 dedup、task_id、stats 全链路。要小心改完不破坏现有"全量扫描"路径——建议保留 `scope=all` 走老逻辑，`scope=dir` 走新逻辑，两条路并存。
2. **`local_path LIKE 'prefix%'` 在大表上慢**。如果 file_records 量大，要给 `local_path` 加索引，或用 `local_path >= prefix AND local_path < prefix_with_high_bound`（range scan，能用索引）。
3. **目录树懒加载**别一次性 `walkdir` 整盘，每个节点展开才 `read_dir` 一层，否则点 C:\ 会卡死。
4. **不要在阶段 A 碰双栏**。双栏的状态管理（两个独立 currentPath/history/selection）、拖拽、焦点切换，会让周期翻倍。

---

## 6. 待你确认的问题（回答后我就开干）

1. **形态**：同意"App 内 `/explorer` 单栏视图"，还是坚持独立 OS 窗口 / 双栏？
2. **范围**：阶段 A 先做浏览器骨架（按钮占位），还是要求 A+B 一起做通整理闭环？
3. **后端定向扫描**：愿意接受"先改后端再上前端"的顺序吗（B 的前置）？还是先出前端 demo，后端凑合用全量扫描（会清库，仅 demo 可接受）？
4. **现有 FileListView**：保留不动？还是最终想用 ExplorerView 替换它？

我倾向：**形态=App 内单栏，A 先做骨架，B 的后端先改，FileListView 暂留**。你拍板。
