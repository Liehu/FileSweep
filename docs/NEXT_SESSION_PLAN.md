# FileSweep 后续开发计划与会话交接文档

**最后更新**：2026-06-25
**当前 HEAD**：`290fcc4 docs: 后续开发计划`（本次会话改动尚未提交）

---

## 本次会话完成（2026-06-25）

### ✅ 任务 1：统计为 0 修复（已加 wal_checkpoint + 单测验证）
- **根因**：`batch_insert_file_records` 在独立连接写入后，WAL 未及时 checkpoint，
  随后 `get_file_stats`/`get_file_records` 打开新独立连接可能读不到最新数据。
- **修复**：`batch_insert_file_records` commit 后加 `PRAGMA wal_checkpoint(TRUNCATE)`，
  强制把 WAL 合并到主库（`db/catalog.rs`）。
- **验证**：新增 `test_batch_insert_then_stats_via_independent_connection` 单测，
  断言写入后独立连接立即读到正确统计 + 二次全量替换后同步更新。**已通过**。
- 全部 19 个 lib 单测通过。

### ✅ 任务 2：建议执行清理闭环
- **改动文件**：`stores/files.ts` + `views/SuggestionPanel.vue`
- **实现**：
  - `useFilesStore` 新增 `executeSuggestionCleanup()`（suggestion→delete 映射）+ `cleanState`/`cleanResult` 状态
  - `clean_complete`/`clean_error` 事件监听更新 cleanState 并刷新建议
  - SuggestionPanel："执行清理"按钮绑定 → 确认对话框（回收站提示）→ 调 clean:start → 结果横幅（删除/移动/失败计数）
  - 新增"全选/取消全选"按钮；首次加载自动勾选 `auto_checked` 项
  - suggestion 映射：`downgrade`/`delete_old`/`delete_dup` → 后端 `delete`

### ✅ 任务 3：AI download_reliability 字段
- **改动文件**：`models.rs` / `migrations.rs` / `catalog.rs`（4 处 SQL）/ `enricher.rs` / `enrich.rs`（2 处）/ `offline.rs` / `cli/main.rs` / `suggestion.rs`
- **实现**：
  - migration patch：`catalog_entries.download_reliability TEXT DEFAULT ''`
  - `CatalogEntry` + `EnrichResult` 加 `download_reliability` 字段
  - catalog CRUD（insert/get/get_by_id/update）4 处 SQL 全部含新列
  - enricher prompt 加 reliability 判断指令；解析时归一化为 high/medium/low/""（非法值清空）
  - offline enricher 视为 high；default 空串
  - `suggestion.rs`：降级建议综合内置知名度表 + AI reliability 判定
    （high→高置信自动勾选，medium→需确认，low→提示先备份）
- **运行时验证**：CLI 启动日志确认 `数据库补丁已应用: table=catalog_entries column=download_reliability`

### ✅ 任务 4：Everything 前端搜索框
- **改动文件**：`stores/files.ts` + `views/FileListView.vue`
- **实现**：
  - `useFilesStore` 新增 Everything 搜索状态（query/results/source/searching/error）+ `searchEverything()`/`clearEverythingSearch()`
  - 自动识别返回格式：Everything 成功返回 `SearchResult[]`；DB 回退返回 `{results,total,source:"database"}`
  - FileListView 新增独立"全局搜索"卡片：400ms 防抖输入 + 来源标签（Everything/数据库回退）+ 结果列表（名称/路径/大小）
  - DB 回退时提示用户安装 Everything + ES CLI 可获全盘结果

### 验证状态
- `cargo check`：通过（仅预先存在的 unused import warnings）
- `cargo test --lib`：19 passed / 0 failed
- `npm run build`：通过（vue-tsc + vite，8.3s）

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
- `devUrl: http://localhost:1420`（注意端口，非 5173——后者在 Windows 动态端口排除范围 5088-5187）
- `frontendDist: ../dist`
- 无标题栏（`decorations: false`）

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
- `devUrl: http://localhost:1420`（注意端口，非 5173——后者在 Windows 动态端口排除范围 5088-5187）
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
| 后续 | 统计为 0 修复（wal_checkpoint） | ✅ 本次 |
| 后续 | 建议执行清理闭环（SuggestionPanel） | ✅ 本次 |
| 后续 | AI download_reliability 字段 | ✅ 本次 |
| 后续 | Everything 前端搜索框 | ✅ 本次 |
| 后续 | Downloads 分类准确性修复（classify_functional 子串误匹配 + 命名空间对齐） | ✅ 2026-07-06 |
| 后续 | 按功能用途整理到细分目录（functional_category → target_path move 建议） | ✅ 2026-07-06 |

---

## 本次会话完成（2026-07-06）

### ✅ 任务 1：修复 Downloads 分类误匹配（classify_functional 子串匹配 bug）

**根因**：`classify_functional` 用 aho-corasick 做**子串匹配**，且 func_categories 种子关键词含大量泛词
（`Windows`/`ISO`/`Directory`/`AES`/`Text`/`Code`），导致：
- `AliCloud-Tools-v1.0.5-windows-amd64.zip` → ISO（"Windows" 子串命中，84 个文件被误分到 ISO）
- `DropIt_*_portable.zip` → IoT-Wireless
- `Directory Opus*` → FileMgr
实测：Downloads 479 条记录全是这个分类器的产出（catalog_entries 0 行 = AI 从没跑过）。

**修复**（3 处联动）：
1. **`core/classifier.rs`**：aho-corasick 子串匹配 → **token 级精确匹配**（仅按分隔符拆 token，不拆驼峰以保留 EasyBCD/Metasploit 等专有名整体）。叠加：关键词最小长度 ≥3（过滤 VS/IDE/JS）、**安全类（parent=="网络安全"）多信号**（要求 ≥2 个不同 token 命中，或唯一命中的是 ≥5 字符纯字母强专有名，避免单 AES 弱信号误判）。
2. **`db/migrations.rs`**：`extract_keywords` stopwords 扩展——新增 `iso`/`directory`/`total`/`commander`/`text`/`code`/`editor`/`player`/`manager`/`photo`/`music`/`video` 等通用英文字典词，以及 `aes`/`des`/`sm4`/`rsa`/`ecc` 通用算法缩写。种子入口加 v2 日志标记。
3. **分类命名空间对齐**：`commands/enrich.rs` 的 `load_func_category_names` 从读 `categories.yaml`（中文名）改为读 DB `func_categories` 表（英文缩写如 `Exp-Frameworks`），让 LLM prompt 的合法分类列表与系统其余部分一致。`ai/offline.rs` 的 `query_db` 返回时 `functional_category` 置空（offline 粗类 network/dev/Security 与 DB 命名空间不一致，留空由分类器/LLM 兜底）。

**验证**：新增 `test_classify_functional_no_substring_false_positive`（5 个回归场景）+ `test_extract_keywords` 泛词过滤断言。48 单测全过。

### ✅ 任务 2：按功能用途整理到细分目录（functional_category → target_path 落地）

**背景**：`func_categories.target_path`（如 `Security\Exploit\Frameworks`）在 DB 里存着但 executor/suggestion/clean **从不读取**，"按功能用途分类到细分目录"功能根本没实现。

**修复**：
- **`core/suggestion.rs`**：`generate_suggestions` 新增 `func_categories: &[FuncCategoryRow]` 参数。新增分支 0.5：文件已有 `functional_category` 且该分类有 `target_path` → 生成 move 建议搬到 `fc.target_path`。置信度：AI confidence≥0.8 或 download_reliability==high → medium；否则 low（均放入 medium 分组带标签）。不自动勾选（移动不可逆，与目录迁移一致）。
- **`plugins/filesweep/actions.rs`**：调用 `generate_suggestions` 时从 `db.list_func_categories()` 取表传入。
- **executor/clean.rs 无需改动**：`parse_frontend_actions` 已支持 `move`+`move_target`，`resolve_dest` 已把相对路径拼到 `migrate_root_dir`（`D:\Sorted`）。

### 验证状态
- `cargo check --lib`：通过（26 warnings，均为预先存在的 unused 项）
- `cargo test --lib`：48 passed / 0 failed（新增 5 个分类器回归测试 + 5 个种子泛词断言）

### ⚠️ 运行时验证（需用户执行）
1. **删 DB 重建**（现有 catalog.db 的 func_categories 是 v1 脏数据）：`del "C:\Users\Spence\AppData\Roaming\FileSweep\config\catalog.db*"`
2. `npm run tauri dev` → 扫描 `D:\Users\Spence\Downloads`
3. **跑 AI 补全**（SuggestionPanel → provider 选 custom → 执行）→ 确认 catalog_entries 有数据
4. 查 SuggestionPanel 出现"功能分类 XXX，建议整理到 Security\..."类 move 建议
5. 注意：OpenRouter free 模型（qwen3-next-80b）有限流，429 时 enricher 会重试 3 次

---

## 本次会话完成（2026-07-07）

### ✅ 任务 1：AI 补全加速——批量文件名列表 + 真并发

**根因**：`batch_enrich` 是假并发（注释自承认"串行处理 + Semaphore 限流"），479 文件串行 HTTP × 每次 2-8s = 30+ 分钟。每文件一次请求，重复传 system prompt + 建连接。

**修复**：
- **`Enricher` trait 加 `enrich_batch`**（带默认实现，offline/claude/ollama/fallback 零改动）：默认串行调 `enrich`。
- **批量 prompt + 解析**（`ai/enricher.rs`）：抽 `parse_one_obj` 公共字段提取；`build_batch_system_prompt` 要求返回 JSON 对象 `{"0":{...}}`（索引作 key，比数组鲁棒）；`build_batch_user_message` 序列化文件列表；`parse_batch_response` 解析并补齐缺失 index。
- **`OpenAIEnricher` 真批量**（`ai/openai.rs`）：一次请求处理整批，`max_tokens = min(4096, 200*batch_len)`（free 模型输出上限 ~4096）；批解析失败/空结果过多 → 降级单文件逐个调。
- **`batch_enrich` 真并发**（`ai/enricher.rs`）：`futures::stream::buffer_unordered(concurrency)`，各批 index 不重叠按 index 写入。
- **`Config.ai_batch_size`**（默认 20）：`models.rs` 加字段 + `config.rs` 默认值 + 规范化（0→20）。`default_concurrency` 从 4 改 2（free 模型 8 req/min 限流）。

**效果**：479 文件 → 24 批，并发 2 → 约 6-8 分钟（较串行 30+ 分钟快 4 倍）。新增 6 个单测（batch prompt/解析/降级）。

### ✅ 任务 2：AI 补全中断 + 续传（增量落库）

**根因**：旧实现"全部批次跑完才一次性落库"（`start_enrich_headless` 第 438-486 行），中途打断 = 已完成批次结果全丢，且无断点续传。

**修复**（镜像 scan 的 `AtomicBool` 中断模式）：
- **中断标志**（`commands/enrich.rs`）：`static ENRICH_CANCEL: AtomicBool` + `request_enrich_cancel()` + `is_enrich_cancelled()`。`start_enrich_headless`/`start_enrich` 开头 reset。
- **中断检查点**：`batch_enrich` 流循环（收到信号停止调度新批次）+ `OpenAIEnricher::enrich_batch` 单文件降级循环（停止重试）+ `call_api` 重试循环（停止傻等退避）。
- **增量落库**：`batch_enrich` 加 `on_batch: impl Fn(&[(usize, EnrichResult)])` 回调，每批完成**立即落库**（insert_catalog_entry + update_file_functional_category）。两入口（headless + 废弃 command）均改为闭包模式，中断 0 丢失。
- **续传**：启动时查 `catalog_entries` 的 name 集合（`ai_provider` 非空 = 已成功丰富），过滤循环跳过已丰富文件。中断后重启自动从断点继续，无需进度表（`catalog_entries.name` UNIQUE）。
- **`enrich:cancel` action** 注册（`actions.rs` + `mod.rs`），镜像 `scan:cancel`。
- **前端**（`SuggestionPanel.vue`）：enriching 时显示"中断"按钮 → `pluginInvoke("filesweep", "enrich:cancel")`；监听 `enrich_cancelled` 事件提示"已保存 X 个，可重新开始继续"。

### ✅ 任务 3：429 限流优化（指数退避）

**根因**：旧 `call_api` 固定 5s × 3 次重试，OpenRouter free 模型限流期间 15s 后放弃，整批失败。且 `buffer_unordered(2)` 下并发批次各自重试会**同时撞 429**，无全局退避。

**修复**（`ai/openai.rs` `call_api`）：
- 重试上限 3→**5 次**（free 模型限流窗口长）。
- 429 退避：固定 5s → **指数退避 5/10/20/40/60s**（取 retry-after header 与退避的较大值）。
- 网络错误退避：固定 2s → 指数 2/4/8s。
- 中断检查：每次重试前查 `is_enrich_cancelled`，避免中断时还在傻等退避。

### 验证状态
- `cargo check --lib`：通过（26 warnings，均为预先存在的 unused 项）
- `cargo test --lib`：54 passed / 0 failed
- `npm run build`：通过（vue-tsc + vite，8.6s）

### ⚠️ 运行时验证（需用户执行）
1. 删 DB 重建：`del "C:\Users\Spence\AppData\Roaming\FileSweep\config\catalog.db*"`
2. `npm run tauri dev` → 扫描 Downloads → SuggestionPanel → provider 选 custom → 开始丰富
3. **测中断**：跑到一半点"中断"→ 查 catalog_entries 有部分数据 → 提示"已保存 X 个"
4. **测续传**：重新点"开始丰富"→ 日志见"跳过 N 个已丰富文件"→ 从断点继续
5. **测 429**：观察日志"OpenAI 429 ... backoff 5/10/20s"→ 退避后重试成功（而非放弃）

---

## 本次会话完成（2026-07-07 续）

### ✅ 任务 1：修复自定义 AI 配置保存不生效（"保存后页面立刻回旧值"）

**根因**：`plugins/filesweep/actions.rs` 的 `settings:update` 分发层把 `ctx.config.read().clone()` 包进**临时** `Arc<tokio::RwLock>` 传给 `update_settings_headless`，后者 `*config.write().await = cfg` 写的是临时锁，**全局 `ctx.config`（`start_enrich` / `settings:get` 读的）从未更新**。
- `config.yaml` 写对了（`cfg.save()` 正常）。
- 但内存 `ctx.config` 是旧值 → `start_enrich` 用旧 key/model；`settings:get` 返回旧值 → 任何 refetch 覆盖前端乐观更新 → 页面回旧值。
- 根因是类型不匹配：`ctx.config` 是 `Arc<parking_lot::RwLock>`，headless 函数要 `&Arc<tokio::RwLock>`，分发层 clone+重包规避类型却丢了全局写回。

**修复**（`actions.rs` `settings:update` 分发）：跑完 `update_settings_headless` 后，把临时 `tok_cfg` 的更新结果写回全局：
```rust
let updated_cfg = tok_cfg.read().await.clone();
*ctx.config.write() = updated_cfg;
```
这样 `ctx.config` 与 `config.yaml` 同步。

### ✅ 任务 2：新增 AI 配置测试功能（连通性+认证）

**后端**：
- `OpenAIEnricher::test_connection`（`ai/openai.rs`）：发极简 ping 请求（"reply ok"/"ping"，max_tokens=10），**不重试**快速失败。成功返回 `model|latency_ms`，失败返回 `HTTP {status}: {body}`（服务端原文透传）。
- `commands::settings::test_ai_connection(args)`（`settings.rs`）：根据 provider 分支——custom/openai 走 OpenAI 兼容、ollama 走 `GET /api/tags`、claude 走 OpenAI 兼容封装、offline 直接成功。校验 key/model 非空。返回 `{ ok, model, latency_ms }` 或 `{ ok: false, error }`。
- 注册 `settings:test` action（`actions.rs` + `mod.rs`）。

**前端**：
- `stores/settings.ts` 加 `testConnection(data)` + `AiConfig` 补 `openai_model` 字段。
- `SettingsView.vue`：保存按钮旁加"测试连接"按钮 → 用**当前表单值**（不必先保存）发 ping；成功显示绿色"✓ 连接成功（model, Xms）"，失败显示红色"✗ 服务端报错：{原文}"。

### 验证状态
- `cargo check --lib`：通过（26 warnings，均为预先存在的 unused 项）
- `cargo test --lib`：54 passed / 0 failed
- `npm run build`：通过

### ⚠️ 运行时验证（需用户执行）
1. 设置页填自定义 AI 配置（url/key/model）→ 保存 → **重进设置页**→ 表单应显示新值（不再回旧值）
2. 填完配置点"测试连接"→ 成功显示"✓ 连接成功（model, Xms）"；故意填错 key → 显示"✗ 服务端报错：HTTP 401: ..."
3. 开始丰富 → 日志确认用的是新配置（不是旧 key/model）

---

## 本次会话完成（2026-07-07 续 2）

### ✅ 任务：修复 reasoning 模型 content=null 导致"unexpected response format"

**根因**（cURL 实测定位）：`tencent/hy3:free` 是推理模型，OpenRouter 返回 `"message": {"content": null, "reasoning": "..."}`。`call_api` 提取 `/choices/0/message/content` 得到 null → 报 "unexpected response format" → 该文件丰富失败。
- 原因：max_tokens（单文件 500 / 批量 4000 / test 10）被 reasoning（思考过程）消耗殆尽，content 没生成（`finish_reason: length`）。

**修复**（三层防御，`ai/openai.rs`）：
1. **请求 body 加 `reasoning: {exclude: true}`**：OpenRouter 参数，让模型仍可内部推理但不返回 reasoning tokens，省 token 给 content。对非 reasoning 模型/原生 OpenAI 无害（忽略未知参数）。
2. **加大 max_tokens**：test_connection 10→200、单文件 enrich 500→1500、批量 enrich `min(4096,200*batch_len)`→`min(8192,400*batch_len)`。
3. **解析容错**：content 为 null 时记录原始 response body 到日志（而非泛泛"unexpected format"），便于诊断。

**cURL 实测验证**（同 key 同模型）：
- 修复前：`Goose-win32-x64.zip` → content=null → "unexpected response format"。
- 修复后：content 非 null，PARSED OK，返回 `{description, license, confidence, download_reliability}` 全字段，`finish: stop`。

### 验证状态
- `cargo test --lib`：54 passed / 0 failed
- cURL 实测 hy3:free：reasoning exclude + max_tokens 加大后 content 非 null，JSON 合法

---

## 本次会话完成（2026-07-07 续 3）

### ✅ 任务：功能分类仅对软件类运行，非软件类不做分类

**问题**：文档类（.md/.docx/.pdf/.txt）被误分到 SysEnhance/DocView 等软件分类。
实测：`MSA_Design_Doc_v3.md` → DocView、`V8.3软件下载教程.docx` → DocView、`AI赋能攻击面探测系统.docx` → Wiki-Recon、`附件1-...docx` → Other（SysEnhance\Other）。共 21 条文档被误分。

**根因**：`classify_functional` 对**所有**文件运行关键词匹配，文档类文件的 token（如 "doc"）会命中软件分类的关键词。

**修复**（`core/classifier.rs`）：`classify_functional` 开头加**门控**——仅对软件类文件运行功能分类：
- 绿色软件目录（`is_app_dir == true`）→ 通过。
- 软件扩展名（安装包 .exe/.msi、压缩包 .zip/.7z、Java .jar、镜像 .iso）→ 通过。
- 其余（文档 .md/.docx/.pdf、媒体 .mp4/.mp3、图片 .png、脚本 .py）→ 直接返回 None。

新增 `is_software_file(ext)` 辅助函数，扩展名列表与 `default_rules` 的软件类 CategoryRule 一致。

**验证**：新增 `test_classify_functional_skips_non_software`（文档类不分类、软件类仍分类、绿色软件目录跳过门控）。更新 `test_classify_functional` 的 f2（.cfg→.exe 通过软件门控）。

### 验证状态
- `cargo test --lib`：55 passed / 0 failed

### ⚠️ 运行时验证
删 DB 重建 → 重扫 Downloads → 查 file_records：文档类（.md/.docx/.pdf）的 `functional_category` 应为空（不再被分到 SysEnhance/DocView）。

---

## 本次会话完成（2026-07-07 续 4）

### ✅ 任务：GitHub 搜索增强 AI 丰富准确性

**思路**（用户提出，cURL 实测验证）：文件多是 GitHub 下载的原始名（用户不改名），先搜 GitHub 拿仓库事实（描述/topics/stars），塞进 enrich prompt 作"已知事实"，AI 基于事实做功能分类，准确性大幅提升。
- 实测：`BehinderClientSource-master.zip` → 命中 `MountCloud/BehinderClientSource ★946`（冰蝎客户端）；`BeeCount-main (1).zip` 去 `(1)` → 命中 `TNT-Likely/BeeCount ★1892`。

**实现**：
- **新模块 `ai/github_search.rs`**（~250 行）：
  - `normalize_filename_for_search`：**只去**重复后缀 ` (1)`/` (2)`（中英文括号），**不去**版本号/平台后缀（用户不改名，原文匹配最精确）。
  - `should_search`：跳过纯十六进制 ≥16 字符（哈希名）、纯数字。
  - `GitHubSearcher`：调 `/search/repositories`，带 `User-Agent: FileSweep` + 可选 Bearer token，429/403 时读 `X-RateLimit-Reset` 退避重试。
  - `find_best_match` + `score_candidate`：评分选最优（名字匹配分 1.0/0.8/0.4/0.3 + stars 加成），>0.5 才采纳。区分"repo name 是 query 前缀"（强 0.8）vs"query 是 repo name 前缀"（中 0.4，防 PixPin→pixpin-manager 误匹配）。
- **EnrichRequest 加 `github_hint`**：`build_user_message`/`build_batch_user_message` 附 GitHub 事实段，system prompt 指示"用 description/topics 判分类，但不照抄 URL"。
- **enrich.rs 整合**：`start_enrich_headless` 在 LLM 阶段前先串行搜 GitHub（受 30/min 限流 + 中断检查），命中填 hint。
- **Config**：`enable_github_search`（默认 true）+ `github_token`（可选，填了走认证 30/min）。
- **前端**：SettingsView AI 卡片加 GitHub 搜索开关 + token 输入；saveAiSettings 同时保存这两个字段。

**约束**：GitHub Search API 认证 30 req/min、未认证 10 req/min。479 文件建议填 token（约 16 分钟）。哈希名/纯中文跳过，省配额。

### 验证状态
- `cargo test --lib`：62 passed / 0 failed（新增 7 个 github_search 单测：normalize/should_search/score 各场景）
- `npm run build`：通过
- cURL 实测：BehinderClientSource/BeeCount/PixPin 匹配质量验证

### ⚠️ 运行时验证
1. 设置页填 GitHub token（建议 fine-grained PAT 只读 public）→ 保存
2. 跑 enrich → 日志见 `enrich: GitHub 搜索增强已启用（已认证 30/min），开始搜索 N 个文件`
3. 命中日志：`GitHub 命中 'BehinderClientSource-master' → MountCloud/BehinderClientSource ★946 (score 1.0)`
4. 完成：`GitHub 搜索完成，X/N 命中` → LLM 阶段用 hint 提升准确性

---

## 3. 未完成任务清单

### 高优先级（功能闭环）

#### 3.0 目录分类两层方案（最新设计，待实现）
- **设计文档**：`docs/superpowers/specs/2026-06-25-dir-classification-design.md`
- **核心**：层 1 目录类型识别（file_markers → dir_name_keywords → 文件类型指纹）+ 层 2 处理策略
- **新增 DB 表**：`dir_patterns`（用户自定义目录模式，内置 9 种默认模式）
- **新增模块**：`core/dir_classifier.rs`（classify_dir_type）
- **集成点**：scanner.rs 四阶段扫描的 mark 后、find_app_roots 前插入层 1
- **覆盖场景**：代码项目/CTF题目/安全知识库/样本集合/培训资料/漏洞资料/Markdown笔记/POC库/临时文件
- **关键信号**：目录名关键词（安全场景强信号）+ 标志文件 + 类型占比 + 无意义文件名检测
- **实现任务**：见设计文档 §7（9 个任务）

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
2. **vite devUrl**：`tauri.conf.json` 和 `vite.config.ts` 都是 `1420`（5173 在 Windows 动态端口排除范围 5088-5187 内，会 EACCES）
3. **DB 膨胀**：多次大目录扫描后 catalog.db 可能膨胀（已用 DELETE + 重建索引缓解，但无 VACUUM）
4. **appmover 托盘**：lib.rs 在 setup 中初始化 appmover 托盘，如果 appmover 有问题会影响启动

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
| **目录分类两层方案（最新）** | `docs/superpowers/specs/2026-06-25-dir-classification-design.md` |
| 归档旧设计文档 | `docs/archive/FileSweep_DesignDoc_v1.0.md` |

---

## 7. 建议的下次会话工作顺序

1. **目录分类两层方案**（§3.0）— 最新设计的核心功能，设计文档完整就绪
2. **验证统计为 0 修复**（删 DB → 重扫 → 检查统计）
3. **实现建议执行清理**（SuggestionPanel 勾选 → clean:start）
4. **加 AI download_reliability 字段**（migration + prompt + suggestion 集成）
5. **Everything 前端搜索框**
6. **appmover 插件编译验证 + 功能测试**
7. **独立窗口模式**（Tauri WebviewWindow）
8. **headless 模式适配**（headless.rs 的 scan/enrich 调用更新签名）
9. **旧 commands 清理**（确认前端无调用后移除）
10. **YAML 导入备份标记**（config_initialized 标志位）
11. **命令面板 fuzzy 搜索**（fzf-style）
