# P2 filesweep 完整迁移设计

**日期**：2026-06-19
**状态**：已批准
**关联文档**：
- `docs/superpowers/specs/2026-06-18-p1-plugin-skeleton-design.md`（P1 插件骨架）
- `docs/superpowers/specs/2026-06-18-plugin-platform-migration-design.md`（总设计）

---

## 1. 目标与范围

### 1.1 目标
让 `plugin_invoke` 通道完全取代旧 `invoke`，filesweep 所有功能走插件分发器。补齐 P1 遗留的复杂 action 与缺失功能，stores 全量迁移到 `pluginInvoke`。

### 1.2 P2 交付
- DB schema 扩展：file_records 增加 action / move_target 列（补齐重构遗漏功能）
- 新增 db 方法 + 3 个文件操作 action（set_action / set_move_target / batch_set_action）
- 新建 clean/enrich headless 版本，适配 scan:start，补齐 4 个复杂 action
- stores 9 处 invoke → pluginInvoke 全量迁移
- nav badge 通用化

### 1.3 不在 P2 范围
- 删除旧 26 个 command 函数（保留作向后兼容 + CLI 入口，P3 评估）
- 新插件开发（dev_env 等，留 P4）
- 动态加载（留 P5）

---

## 2. 探索发现（关键事实）

| 项 | 现状 |
|---|---|
| 21 个简单 action | ✅ P1 已实现 |
| `scan:start` | 有 `start_scan_headless`，但签名消费 `db: CatalogDB` + 需 `broadcast::Sender` |
| `clean:start` | command 函数内联逻辑（spawn + emit），**无 headless 版本** |
| `enrich:start` / `enrich:status` | 依赖 enrich_state 共享状态，**无 headless 版本** |
| `set_file_action` / `set_move_target` / `batch_set_action` | **完全缺失**——后端无函数、DB 无 action/move_target 列、UI 却在调用（FileListView 批量操作/单文件 action 选择） |

⚠️ **重大发现**：`set_file_action` 等 3 个命令是 Go→Tauri 重构时遗漏的功能缺失。file_records 表无 action/move_target 列，UI 引用但后端不存在，调用即运行时失败。P2 必须补齐才能让清理流程（标记→批量执行）可用。

### 2.1 关键签名（已核实）

`start_scan_headless`（scan.rs）：
```rust
pub async fn start_scan_headless(
    db: CatalogDB,                    // 消费 db，需改为 Arc<CatalogDB>
    config: Arc<Config>,
    dirs: Vec<String>,
    recursive: bool,
    exclude_dirs: Vec<String>,
    exclude_names: Vec<String>,
    exclude_exts: Vec<String>,
    detect_app_dirs: bool,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<Value, String>
```

`start_clean` command（clean.rs）：接受 `app: AppHandle, db, config, confirm, file_actions`，在 tokio::spawn 内执行，通过 `app.emit` 广播 clean_complete/clean_error/clean_progress 事件。

`start_enrich` command（enrich.rs）：接受 `app, db, config, enrich_state` + 业务参数，spawn 内执行，emit enrich_progress/enrich_complete。

### 2.2 stores invoke 用法（9 处）

- catalog.ts: `update_catalog_entry`, `delete_catalog_entry`
- files.ts: `start_scan`, `set_file_action`, `set_move_target`, `batch_set_action`, `start_clean`
- settings.ts: `update_settings`(×2), `reset_db`

---

## 3. 后端设计

### 3.1 DB schema 扩展（migrations.rs）

新增 migration：
```rust
"ALTER TABLE file_records ADD COLUMN action TEXT DEFAULT ''",
"ALTER TABLE file_records ADD COLUMN move_target TEXT DEFAULT ''",
```

> SQLite 的 ALTER TABLE ADD COLUMN 是幂等性需注意——若列已存在会报错。migration 框架需支持「已应用则跳过」。检查现有 migrations.rs 的 migration 应用机制（按版本号/序号追踪），追加新 migration 条目。

### 3.2 新增 db 方法（catalog.rs）

```rust
impl CatalogDB {
    /// 设置单文件的清理动作（delete/keep/move）及移动目标
    pub fn set_file_action(&self, id: &str, action: &str, move_target: Option<&str>) -> SqlResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE file_records SET action = ?, move_target = ? WHERE id = ?",
            params![action, move_target.unwrap_or(""), id],
        )?;
        Ok(())
    }

    /// 批量设置清理动作
    pub fn batch_set_action(&self, ids: &[String], action: &str, move_target: Option<&str>) -> SqlResult<usize> {
        let conn = self.conn.lock().unwrap();
        let mut count = 0;
        for id in ids {
            conn.execute(
                "UPDATE file_records SET action = ?, move_target = ? WHERE id = ?",
                params![action, move_target.unwrap_or(""), id],
            )?;
            count += 1;
        }
        Ok(count)
    }

    /// 查询所有已设置 action 的文件（供 clean 使用）
    pub fn get_files_with_actions(&self) -> SqlResult<Vec<FileRecord>> {
        let conn = self.conn.lock().unwrap();
        // 查询 action != '' 的记录
        // ...
    }
}
```

> FileRecord 结构体需同步增加 `action: String` 和 `move_target: String` 字段（带 `#[serde(default)]`）。

### 3.3 scan:start 适配（修改 start_scan_headless 签名）

将 `db: CatalogDB` 改为 `db: Arc<CatalogDB>`（不消费），函数体内 db 用法通过 deref 不变。

### 3.4 新建 start_clean_headless（clean.rs）

```rust
pub async fn start_clean_headless(
    db: Arc<CatalogDB>,
    config: Config,
    confirm: bool,
    file_actions: Vec<serde_json::Value>,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), String> {
    // 提取自 start_clean command 的 spawn 内核心逻辑
    // 原 app.emit(...) 改为 event_tx.send(...)
    let dry_run = !confirm;
    let actions = if file_actions.is_empty() {
        // 自动模式：从 db 读 get_files_with_actions() 或生成
        ...
    } else {
        // 手动模式：解析 file_actions
        ...
    };
    // 执行清理，通过 event_tx 广播进度
    ...
    let _ = event_tx.send(serde_json::to_string(&serde_json::json!({"type":"complete"}))?);
    Ok(())
}
```

### 3.5 新建 start_enrich_headless / get_enrich_status_headless（enrich.rs）

```rust
pub async fn start_enrich_headless(
    db: Arc<CatalogDB>,
    config: Config,
    enrich_state: Arc<parking_lot::Mutex<EnrichState>>,
    params: EnrichParams,
    event_tx: tokio::sync::broadcast::Sender<String>,
) -> Result<(), String> {
    // 提取自 start_enrich command 的 spawn 内核心逻辑
    // enrich_state 作为参数传入（无 Tauri State 依赖）
    // app.emit 改 event_tx.send
    ...
}

pub fn get_enrich_status_headless(
    enrich_state: &Arc<parking_lot::Mutex<EnrichState>>,
) -> EnrichStatus {
    let state = enrich_state.lock();
    EnrichStatus {
        running: state.running,
        processed: state.processed,
        total: state.total,
        ...
    }
}
```

> enrich_state（SharedEnrichState = Arc<parking_lot::Mutex<EnrichState>>）需纳入 Context，使 plugin_invoke 能访问。Context 增加 `enrich_state` 字段。

### 3.6 Context 扩展

```rust
pub struct Context {
    pub db: Arc<CatalogDB>,
    pub config: Arc<RwLock<Config>>,
    pub app_handle: tauri::AppHandle,
    pub enrich_state: Arc<parking_lot::Mutex<commands::enrich::EnrichState>>,  // 新增
}
```

lib.rs 的 plugin_invoke 命令注入 enrich_state；manage enrich_state 不变。

### 3.7 broadcast → app.emit 桥接（actions.rs）

所有长任务 action（scan:start / clean:start / enrich:start）统一模式：
```rust
"scan:start" => {
    let (tx, mut rx) = tokio::sync::broadcast::channel::<String>(16);
    let app = ctx.app_handle.clone();
    tokio::spawn(async move {
        while let Ok(ev) = rx.recv().await {
            let _ = app.emit("scan_event", ev);
        }
    });
    let config = ctx.config.read().clone();
    commands::scan::start_scan_headless(ctx.db.clone(), Arc::new(config), ..., tx).await?;
    Ok(Value::Null)
}
```

> 事件名约定：前端 listen 原有事件名（scan_complete/clean_complete/enrich_complete 等）。headless 函数发送的 JSON 含 `type` 字段区分，桥接层转发原事件名。需对照前端 listen 的事件名调整。

### 3.8 actions.rs 新增 action

- `files:set_action`（args: fileId, action, moveTarget）→ `db.set_file_action`
- `files:set_move_target`（args: fileId, target）→ `db.set_file_action(id, "", Some(target))` 或独立方法
- `files:batch_set_action`（args: fileIds, action, moveTarget）→ `db.batch_set_action`

FileSweepPlugin::actions() 列表追加这 3 个。

### 3.9 PluginError 兼容

`From<tokio::sync::broadcast::SendError<String>>`（若需要）。

---

## 4. 前端设计

### 4.1 stores 全量迁移（pluginInvoke）

```ts
// 旧
await invoke("start_scan", { dirs, ... });
// 新
await pluginInvoke("filesweep", "scan:start", { dirs, ... });
```

9 处替换，action 名映射：
- `update_catalog_entry` → `catalog:update`
- `delete_catalog_entry` → `catalog:delete`
- `start_scan` → `scan:start`
- `set_file_action` → `files:set_action`
- `set_move_target` → `files:set_move_target`
- `batch_set_action` → `files:batch_set_action`
- `start_clean` → `clean:start`
- `update_settings` → `settings:update`
- `reset_db` → `db:reset`

### 4.2 nav badge 通用化

NavItem.badge() 机制生效。filesweep nav.ts：
```ts
{ label: "重复文件", icon: "Copy", route: "/files", query: { dup: "1" },
  badge: () => filesStore.stats.duplicates || undefined }
```

Sidebar 改为遍历 NavItem.badge() 调用（而非硬编码 duplicateCount）。Sidebar 需访问 filesStore——通过 props 传入 badge 数据，或 Sidebar 内 useFilesStore（插件 store 耦合问题）。

> 决策：Sidebar 内不直接引用 filesweep store（宿主不应依赖具体插件 store）。改为 AppShell 计算各插件 badge 后传入 Sidebar，或 Sidebar 接受 `badges: Record<string, number|string>` prop。采用后者：AppShell 从 filesStore 读 duplicates 传入。

### 4.3 FileRecord 类型扩展

前端 types（若有 FileRecord 接口）增加 `action?: string` / `move_target?: string`。

---

## 5. 数据流（scan:start 完整链路）

```
ScanView 点击扫描
  → filesStore.startScan(dirs)
  → pluginInvoke("filesweep", "scan:start", {dirs,...})
  → invoke("plugin_invoke", {plugin, action, args})
  → app::ipc::plugin_invoke（构造 Context）
  → PluginHost::dispatch("filesweep", "scan:start", ...)
  → FileSweepPlugin::invoke → actions::dispatch
  → 创建 broadcast channel + spawn 桥接（rx → app.emit）
  → start_scan_headless(db, config, ..., tx)
    └── 扫描中 tx.send(progress) → 桥接层 app.emit("scan_event", ...)
  → 前端 listen("scan_event") 更新进度
  → 完成 tx.send(complete) → app.emit → 前端 listen("scan_complete")
```

---

## 6. 验收标准

- [ ] `cargo check` 通过
- [ ] `npm run build` 通过
- [ ] DB migration 应用后 file_records 含 action/move_target 列
- [ ] `plugin_invoke("filesweep","files:set_action",...)` 可设置文件 action
- [ ] `plugin_invoke("filesweep","scan:start",...)` 触发扫描并广播事件
- [ ] `plugin_invoke("filesweep","clean:start",...)` 执行清理
- [ ] `plugin_invoke("filesweep","enrich:start",...)` 启动 AI 丰富
- [ ] stores 内 grep 无旧 `invoke("` 残留（全部 pluginInvoke）
- [ ] FileListView 批量操作/单文件 action 功能可用
- [ ] 侧栏「重复文件」badge 显示计数

---

## 7. 风险与缓解

| 风险 | 缓解 |
|---|---|
| DB migration 非幂等（重复应用报错） | 检查 migrations.rs 机制；SQLite 用 PRAGMA user_version 或 migration 表追踪，新增条目自动跳过已应用 |
| broadcast 桥接事件名与前端 listen 不匹配 | 对照前端 listen 的事件名（scan_complete/clean_complete/enrich_complete/progress），桥接层映射 |
| enrich_state 纳入 Context 后 plugin_invoke 签名膨胀 | 接受——enrich_state 是 filesweep 专属，P4 其他插件不用。或 enrich action 不走通用 Context，特化处理 |
| FileRecord 结构体加字段影响序列化 | 用 `#[serde(default)]` 保证旧数据反序列化不失败 |
| stores 迁移后 action 名拼写错误 | 严格按 4.1 映射表；构建时 vue-tsc 会捕获类型错误，运行时手动走查 |

---

## 8. 任务顺序（依赖链）

1. **块1** DB schema + db 方法 + FileRecord 字段（基础）
2. **块2** 新建 clean/enrich headless（依赖块1的 db action 读取）
3. **块3** 适配 scan:start + broadcast 桥接（独立但同模式）
4. **块4** actions.rs 补齐 7 个 action（依赖块1-3）
5. **块5** stores 全量迁移 pluginInvoke（依赖块4所有 action 就绪）
6. **块6** nav badge 通用化
7. **块7** 验证（cargo check + npm run build + grep）
