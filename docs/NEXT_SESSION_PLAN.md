# FileSweep 后续开发计划与会话交接文档

**最后更新**：2026-06-25
**当前 HEAD**：`75ef775 feat(P5): 命令面板增强`

---

## 0. 快速启动指南（新会话必读）

### 环境

- **工作目录**：`D:\Users\Spence\Desktop\FileSweep`
- **Rust 工具链**：`D:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin` + `D:\env\rust\cargo\bin`
- **运行时 DB 路径**：`C:\Users\Spence\AppData\Roaming\FileSweep\config\catalog.db`
- **DB 清理**：`del "C:\Users\Spence\AppData\Roaming\FileSweep\config\catalog.db*"`
- **启动**：`npm run tauri dev`（需要新终端继承 Rust PATH）
- **cargo check**：`pushd src-tauri && set "PATH=D:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;D:\env\rust\cargo\bin;%PATH%" && cargo check && popd`
- **前端构建**：`npm run build`
- **编码警告**：Rust 文件用 Edit 工具修改（不要用 powershell Set-Content，会破坏 UTF-8）
- **git add 注意**：排除 `nul` 文件（Windows 保留名），用 `git add src/ src-tauri/` 而非 `git add -A`

### tauri.conf.json 关键配置

- `withGlobalTauri: true`（DevTools 可用 `window.__TAURI__.core.invoke`）
- `devUrl: http://localhost:5173`（注意端口）
- `frontendDist: ../dist`
- 无标题栏（`decorations: false`）

---

## 1. 项目架构概览

### 技术栈

- **后端**：Rust + Tauri v2 + rusqlite（bundled SQLite）
- **前端**：Vue 3 + TypeScript + vue-router + Pinia + radix-vue(shadcn) + tailwindcss + lucide icons
- **构建**：Vite + vue-tsc

### 插件化架构

```
src-tauri/src/
├── app/              # 宿主内核
│   ├── plugin.rs     # Plugin trait + PluginMetadata + PluginType + FeatureType + PluginPermissions
│   ├── context.rs    # Context（db/config/app_handle/enrich_state）
│   ├── host.rs       # PluginHost 注册表 + dispatch
│   └── ipc.rs        # plugin_invoke / plugin_list Tauri 命令
├── plugins/
│   ├── filesweep/    # 文件整理插件（核心）
│   │   ├── mod.rs    # FileSweepPlugin + manifest
│   │   └── actions.rs # ~75 个 action 分发
│   └── appmover/     # 应用迁移插件（第二个插件，已注册但未完整验证）
│       ├── mod.rs    # AppMoverPlugin
│       ├── actions.rs
│       ├── baseline.rs / describe.rs / envvar.rs / identify.rs
│       ├── migrate/  # copier/killer/locker/planner
│       ├── monitor.rs / tray.rs / uninstall.rs
│       └── models.rs
├── core/
│   ├── scanner.rs    # 四阶段扫描 + scan_software_root
│   ├── appdir.rs     # 绿色软件识别（评分模型 + is_executable_marker + SubtreeStats）
│   ├── classifier.rs # 分类规则引擎（OnceLock 缓存 + DB 读取）
│   ├── suggestion.rs # 智能建议引擎（决策矩阵 + 内置知名度表）
│   ├── everything.rs # Everything SDK 集成（es.exe 调用 + DB 回退）
│   ├── dedup.rs      # 去重检测（版本比对 + 模糊名称）
│   ├── executor.rs   # 文件操作执行器（移动/删除/回收站）
│   ├── config.rs     # Config 结构 + 默认值 + AI 配置
│   ├── models.rs     # FileRecord / CatalogEntry / ScanProgress / DedupGroup 等
│   └── version.rs    # 版本号提取 + 比较
├── db/
│   ├── catalog.rs    # CatalogDB（file_records/catalog_entries/operation_logs/tags）
│   ├── config.rs     # 4 张配置表 CRUD（software_roots/category_rules/func_categories/exclude_rules）
│   └── migrations.rs # 表创建 + patches + 默认数据 + YAML 导入
├── ai/               # AI 丰富（claude/openai/ollama/offline/fallback）
├── commands/         # 原始 Tauri 命令（向后兼容，stores 渐进迁移到 pluginInvoke）
├── headless.rs       # headless 模式（P3 适配遗留）
├── lib.rs            # 入口：构建 PluginHost + 注册插件 + Tauri Builder
└── main.rs           # main（headless 参数解析）

src/  # 前端
├── lib/
│   ├── api.ts        # 统一 invoke/listen（GUI 模式 + headless HTTP 模式）
│   ├── plugin.ts     # definePlugin + PluginManifest + getAllFeatures
│   └── pluginInvoke.ts # pluginInvoke 封装
├── shell/
│   ├── AppShell.vue  # 宿主壳（标题栏 + Sidebar + 右侧面板 + CommandPalette）
│   ├── Sidebar.vue   # 动态侧栏（从插件 navGroups 渲染 + badges）
│   ├── CommandPalette.vue # Alt+Space 命令面板（按 feature 搜索）
│   └── iconMap.ts    # lucide 图标名→组件映射
├── plugins/
│   ├── _registry.ts  # 插件注册汇总
│   ├── filesweep/    # 前端插件
│   │   ├── index.ts  # manifest（7 个 features）
│   │   ├── routes.ts # 8 条路由
│   │   ├── nav.ts    # 侧栏导航
│   │   ├── stores/   # files.ts / catalog.ts / settings.ts（全部用 pluginInvoke）
│   │   └── views/    # FileListView / ScanView / CatalogView / EnrichView /
│   │                 # SuggestionPanel / ConfigView / LogsView / SettingsView
│   └── appmover/     # 前端 appmover 插件
│       ├── views/    # BaselineView / EnvVarView / HistoryView / MigrateView / MonitorView
│       └── stores/appmover.ts
└── App.vue           # 挂载 AppShell
```

### 核心架构决策（踩过的坑）

1. **DB 操作用独立连接**：`rusqlite::Connection::open(&self.db_path)` 而非 Mutex<Connection>，避免 lock 竞争
2. **scan:start 用 tokio::spawn 后台执行**：不阻塞 invoke 通道
3. **scan:status 轮询**：AtomicBool 标志，前端每秒轮询检测扫描完成（Tauri event 不可靠）
4. **DB 读操作用 spawn_blocking**：scan:files/scan:stats 等查询不阻塞 tokio runtime
5. **batch_insert 用 DROP INDEX + DELETE + INSERT + CREATE INDEX**：在独立连接上执行
6. **hash 策略**：partial hash（头尾 4KB + 大小）用于内容文件；元数据 hash 用于 exe/dll
7. **绿色软件识别**：四阶段扫描（collect_dir_tree → mark_subtree_stats → find_app_roots → scan_files）+ 综合评分模型
8. **软件根路径简化扫描**：software_roots 表的路径用 `scan_software_root`（一级目录直接 app dir，不递归）

---

## 2. P0-P5 完成状态

| 优先级 | 内容 | 状态 |
|---|---|---|
| P0 | 修统计 + 清理日志 + 验证 | ✅ |
| P1 | 配置 DB 化（4 表 + CRUD + ConfigView） | ✅ |
| P2 | 软件根路径简化扫描 | ✅ |
| P3 | 智能建议引擎（决策矩阵 + 知名度表 + 分组 UI） | ✅ |
| P4 | Everything SDK 集成（es.exe + DB 回退） | ✅ |
| P5 | 命令面板增强（搜索/建议/配置 feature） | ✅ |

---

## 3. 未完成任务清单

### 高优先级（功能闭环）

#### 3.1 统计为 0 问题
- **现象**：get_file_stats 用独立连接（`Connection::open`）可能读到旧数据
- **已尝试**：独立连接加了 `PRAGMA journal_mode=WAL`
- **待验证**：删 DB 重扫后统计是否正常
- **可能根因**：batch_insert 的独立连接写入后，WAL checkpoint 时机问题
- **修复方向**：batch_insert 完成后加 `PRAGMA wal_checkpoint(TRUNCATE)` 强制刷盘

#### 3.2 AI enrich 加 download_reliability 字段
- **设计**：P3 产品定义中 AI 补全 `download_reliability`（high/medium/low）
- **改动**：
  - `catalog_entries` 表加 `download_reliability TEXT DEFAULT ''`（migration patch）
  - `models.rs::CatalogEntry` 加字段
  - AI prompt 加 reliability 判断指令
  - `suggestion.rs` 的 generate_suggestions 用 reliability 而非只靠内置表
- **影响文件**：models.rs / catalog.rs / enrich.rs / suggestion.rs

#### 3.3 Everything 前端搜索框
- **后端**：`search` action 已就绪（es.exe + DB 回退）
- **前端**：需在 FileListView 或独立页面加搜索输入框
- **依赖**：用户安装 Everything + ES CLI（es.exe 在 PATH）

#### 3.4 智能建议执行清理
- **当前**：SuggestionPanel 显示建议 + 勾选，但"执行清理"按钮未实现
- **需做**：勾选项 → 调 `clean:start` action（已有）→ 删除文件入回收站 + catalog 保留链接

### 中优先级（体验优化）

#### 3.5 独立窗口模式（P5 增强剩余）
- **目标**：命令面板选择 feature 后在新 Tauri WebviewWindow 打开
- **实现**：Tauri `WebviewWindowBuilder` + 各插件的路由在新窗口加载
- **复杂度**：中（窗口管理 + 多窗口路由隔离）

#### 3.6 appmover 插件完善
- **现状**：Rust 后端代码大量已存在（baseline/describe/envvar/identify/migrate/monitor/tray/uninstall）
- **前端**：5 个 View 已存在
- **问题**：功能未完整验证，可能有编译/运行时问题
- **建议**：cargo check 确认编译，逐功能测试

#### 3.7 headless 模式适配
- **现状**：headless.rs 的 scan/enrich 调用返回 "P3 适配" 错误
- **影响**：仅 CLI 模式用户（GUI 正常）
- **修复**：start_scan_headless 签名变更后更新 headless.rs 的调用

#### 3.8 旧 commands 向后兼容清理
- **现状**：lib.rs 仍注册 26 个旧 Tauri 命令（stores 已全量迁移到 pluginInvoke）
- **建议**：确认无前端代码调用旧命令后，移除 generate_handler 中的旧命令

### 低优先级（完善）

#### 3.9 YAML 导入后备份标记
- **现状**：首次启动从 rules.yaml/categories.yaml 导入 DB，但每次启动都检查表为空
- **问题**：如果用户清空 DB 表，重启会重新导入（可能覆盖用户修改）
- **修复**：加 `config_initialized` 标志位，首次导入后设 true

#### 3.10 离线知识库（offline_db.sqlite）
- **现状**：`config/offline_db.sqlite` 存在（旧 Go 设计的 P2 功能）
- **Tauri 端**：`ai/offline.rs` 有 OfflineEnricher 读取它
- **待做**：验证离线丰富是否正常工作

#### 3.11 命令面板 fuzzy 搜索
- **现状**：简单 `includes` 匹配
- **改进**：fuzzy 匹配算法（如 fzf-style）

---

## 4. 已知 Bug / 注意事项

1. **`nul` 文件**：Windows 保留名，`git add -A` 会失败。始终用 `git add src/ src-tauri/` 排除
2. **统计可能为 0**：get_file_stats 独立连接 WAL 时机问题（见 3.1）
3. **vite devUrl**：`tauri.conf.json` 是 `5173`（不是之前的 `1420`），vite.config.ts 需匹配
4. **DB 膨胀**：多次大目录扫描后 catalog.db 可能膨胀（已用 DELETE + 重建索引缓解，但无 VACUUM）
5. **appmover 托盘**：lib.rs 在 setup 中初始化 appmover 托盘，如果 appmover 有问题会影响启动

---

## 5. 关键代码位置速查

| 功能 | 文件 | 关键函数/结构 |
|---|---|---|
| 扫描入口 | `commands/scan.rs` | `start_scan_headless()` |
| 软件根路径扫描 | `core/scanner.rs` | `Scanner::scan_software_root()` |
| 普通目录扫描 | `core/scanner.rs` | `Scanner::scan()`（四阶段） |
| 绿色软件识别 | `core/scanner.rs` | `classify_dir()`（评分模型） |
| 智能建议 | `core/suggestion.rs` | `generate_suggestions()` |
| Everything 搜索 | `core/everything.rs` | `search_with_everything()` |
| DB CRUD（配置） | `db/config.rs` | `SoftwareRoot/CategoryRuleRow/FuncCategoryRow/ExcludeRule` |
| DB CRUD（文件） | `db/catalog.rs` | `CatalogDB::get_file_records/batch_insert/get_file_stats` |
| 配置表 migration | `db/migrations.rs` | `init_default_config() + import_rules_yaml() + import_categories_yaml()` |
| 插件注册 | `lib.rs` | `plugin_host.register(Box::new(...))` |
| 前端 store | `plugins/filesweep/stores/files.ts` | `startScan/fetchFiles/fetchSuggestionsV2` |
| 前端配置页 | `plugins/filesweep/views/ConfigView.vue` | 4 tab（软件根路径/分类规则/功能分类/排除规则） |
| 前端建议面板 | `plugins/filesweep/views/SuggestionPanel.vue` | 摘要→分组→展开 |

---

## 6. 产品设计文档索引

| 文档 | 路径 |
|---|---|
| 产品定位与功能设计（grill 成果） | `docs/superpowers/specs/2026-06-22-product-definition.md` |
| 插件化平台总设计 | `docs/superpowers/specs/2026-06-18-plugin-platform-migration-design.md` |
| P1 插件骨架设计 | `docs/superpowers/specs/2026-06-18-p1-plugin-skeleton-design.md` |
| P2 完整迁移设计 | `docs/superpowers/specs/2026-06-19-p2-filesweep-full-migration-design.md` |
| appdir v2 设计 | `docs/superpowers/specs/2026-06-19-appdir-detection-v2-design.md` |
| appdir 评分模型设计 | `docs/superpowers/specs/2026-06-20-appdir-scoring-design.md` |
| 配置 DB 化设计 | `docs/superpowers/specs/2026-06-22-config-db-migration-design.md` |
| 归档旧设计文档 | `docs/archive/FileSweep_DesignDoc_v1.0.md` |

---

## 7. 建议的下次会话工作顺序

1. **验证统计为 0 修复**（删 DB → 重扫 → 检查统计）
2. **实现建议执行清理**（SuggestionPanel 勾选 → clean:start）
3. **加 AI download_reliability 字段**（migration + prompt + suggestion 集成）
4. **Everything 前端搜索框**
5. **appmover 插件编译验证 + 功能测试**
6. **独立窗口模式**（Tauri WebviewWindow）
7. **旧 commands 清理**（确认无调用后移除）
