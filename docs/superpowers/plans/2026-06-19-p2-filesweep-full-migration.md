# P2 filesweep 完整迁移 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让 plugin_invoke 通道完全取代旧 invoke，filesweep 所有功能走插件分发器；补齐重构遗漏的文件操作功能（action/move_target）与 4 个复杂 action，stores 全量迁移到 pluginInvoke。

**Architecture:** 后端：DB schema 加 action/move_target 列（复用现有 patches 幂等机制）+ 新增 db 方法；为 clean/enrich 新建 headless 版本（参照 start_scan_headless 模式），actions 层通过 broadcast→app.emit 桥接保留原有事件名。前端：stores 9 处 invoke→pluginInvoke，nav badge 通用化。

**Tech Stack:** Rust + Tauri v2 + rusqlite + tokio broadcast；Vue3 + TypeScript + Pinia

**关联设计文档：** `docs/superpowers/specs/2026-06-19-p2-filesweep-full-migration-design.md`

**关键事实（已核实）：**
- migration 机制：`migrations.rs` 的 `patches` 数组 + `column_exists()` 幂等检查，新列直接加入 patches 即可
- 前端 listen 事件名：`scan_complete` / `clean_complete`（files.ts）；command emit 的事件名：scan(`scan_progress`/`scan_error`/`scan_complete`)、clean(`clean_error`/`clean_complete`)、enrich(`enrich_error`/`enrich_complete`/`enrich_progress`)
- `start_scan_headless` 签名消费 `db: CatalogDB`，需改 `Arc<CatalogDB>`
- FileRecord 结构体在 `db/catalog.rs`，需加 action/move_target 字段
- enrich_state 类型 = `Arc<parking_lot::Mutex<commands::enrich::EnrichState>>`（SharedEnrichState）
- 本机 cargo 路径：`D:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin` + `D:\env\rust\cargo\bin`，需 `set PATH=...;%PATH%` 前缀
- 文件编码：用 Edit 工具改 Rust 文件（UTF-8 安全），禁用 powershell Set-Content

---

## 文件结构

**修改（后端）：**
- `src-tauri/src/db/migrations.rs` — patches 数组加 action/move_target
- `src-tauri/src/db/catalog.rs` — FileRecord 加字段 + 3 个新 db 方法
- `src-tauri/src/commands/scan.rs` — start_scan_headless 改 Arc<CatalogDB>
- `src-tauri/src/commands/clean.rs` — 新建 start_clean_headless
- `src-tauri/src/commands/enrich.rs` — 新建 start_enrich_headless + get_enrich_status_headless
- `src-tauri/src/app/context.rs` — Context 加 enrich_state 字段
- `src-tauri/src/app/ipc.rs` — plugin_invoke 注入 enrich_state
- `src-tauri/src/lib.rs` — plugin_invoke 注册加 enrich_state 参数
- `src-tauri/src/plugins/filesweep/mod.rs` — actions() 列表加 3 个新 action
- `src-tauri/src/plugins/filesweep/actions.rs` — 补齐 7 个 action

**修改（前端）：**
- `src/plugins/filesweep/stores/catalog.ts` — invoke→pluginInvoke
- `src/plugins/filesweep/stores/files.ts` — invoke→pluginInvoke
- `src/plugins/filesweep/stores/settings.ts` — invoke→pluginInvoke
- `src/plugins/filesweep/nav.ts` — badge 机制
- `src/shell/Sidebar.vue` — badge prop 通用化
- `src/shell/AppShell.vue` — 传 badge 数据

---

## 阶段划分

- **阶段 A（后端补齐）** Task 1-7。检查点：cargo check。
- **阶段 B（前端迁移）** Task 8-11。检查点：npm run build + grep 无旧 invoke。

---

# 阶段 A：后端补齐

### Task 1: DB schema 扩展 + FileRecord 字段

**Files:**
- Modify: `src-tauri/src/db/migrations.rs`
- Modify: `src-tauri/src/db/catalog.rs`

- [ ] **Step 1: patches 数组加 action/move_target 列**

Read `src-tauri/src/db/migrations.rs`，在 `patches` 数组末尾（`catalog_entries` 的 ai_skip 之后）追加：

```rust
        ("file_records", "action", "TEXT DEFAULT ''"),
        ("file_records", "move_target", "TEXT DEFAULT ''"),
```

完整的 patches 数组末尾应为：
```rust
        ("file_records", "is_app_dir", "INTEGER DEFAULT 0"),
        ("file_records", "app_dir_path", "TEXT DEFAULT ''"),
        ("file_records", "app_dir_reason", "TEXT DEFAULT ''"),
        ("catalog_entries", "ai_skip", "INTEGER DEFAULT 0"),
        ("file_records", "action", "TEXT DEFAULT ''"),
        ("file_records", "move_target", "TEXT DEFAULT ''"),
```

- [ ] **Step 2: FileRecord 结构体加字段**

Read `src-tauri/src/db/catalog.rs`，找到 `pub struct FileRecord` 定义。在 `app_dir_reason` 字段后追加（若有 ai_skip/app_dir 字段则在它们之后）：

```rust
    #[serde(default)]
    pub action: String,
    #[serde(default)]
    pub move_target: String,
```

> 注意：FileRecord 的所有字段需 `#[serde(default)]` 以兼容旧数据。确认现有字段是否已有此属性，保持一致。action/move_target 必须加 `#[serde(default)]`。

- [ ] **Step 3: 更新 FileRecord 的 row 映射**

catalog.rs 中查询 file_records 的地方（`query_as` / 手动 row 映射）需读取新列。搜索 `file_size` 或 `status` 的 row 读取处，确认 SELECT 是否用 `*`（自动含新列）或显式列名（需加 action, move_target）。

若是显式列名 SELECT，找到所有 `SELECT ... FROM file_records` 语句，加 `action, move_target`。若是 `SELECT *`，无需改。

Run: 在 `src-tauri/src/db/catalog.rs` 中搜索 `SELECT` + `file_records`，确认列名写法。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/db/catalog.rs
git commit -m "feat(db): file_records 加 action/move_target 列 + FileRecord 字段"
```

---

### Task 2: 新增 db 方法（set_file_action / batch_set_action / get_files_with_actions）

**Files:**
- Modify: `src-tauri/src/db/catalog.rs`

- [ ] **Step 1: 添加 set_file_action 方法**

在 catalog.rs 的 `impl CatalogDB` 块内（`update_file_status` 方法附近）添加：

```rust
    /// 设置单文件的清理动作及移动目标
    pub fn set_file_action(
        &self,
        id: &str,
        action: &str,
        move_target: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE file_records SET action = ?, move_target = ? WHERE id = ?",
            rusqlite::params![action, move_target.unwrap_or(""), id],
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
```

- [ ] **Step 2: 添加 batch_set_action 方法**

```rust
    /// 批量设置清理动作
    pub fn batch_set_action(
        &self,
        ids: &[String],
        action: &str,
        move_target: Option<&str>,
    ) -> Result<usize, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string())?;
        let mut count = 0;
        for id in ids {
            conn.execute(
                "UPDATE file_records SET action = ?, move_target = ? WHERE id = ?",
                rusqlite::params![action, move_target.unwrap_or(""), id],
            )
            .map_err(|e| e.to_string())?;
            count += 1;
        }
        Ok(count)
    }
```

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/db/catalog.rs
git commit -m "feat(db): 添加 set_file_action / batch_set_action 方法"
```

---

### Task 3: start_scan_headless 改 Arc<CatalogDB>

**Files:**
- Modify: `src-tauri/src/commands/scan.rs`

- [ ] **Step 1: 修改 start_scan_headless 签名**

Read `src-tauri/src/commands/scan.rs`，找到 `pub async fn start_scan_headless(`。

将第一个参数 `db: CatalogDB,` 改为 `db: Arc<CatalogDB>,`。

确保文件顶部有 `use std::sync::Arc;`（scan.rs 已有，Task 7 P1 加过）。

函数体内所有 `db.` 调用通过 Arc deref 自动工作，无需改。

- [ ] **Step 2: 确认 start_scan command 调用处兼容**

start_scan command 函数（`#[tauri::command]`）内部调用 `start_scan_headless`。检查它如何传 db——应改为 `db.inner().clone()`（得 Arc<CatalogDB>）。

Read start_scan command 函数体，确认传参。若原来传 `db.inner().clone()`（旧 CatalogDB clone），现在 db 是 `State<Arc<CatalogDB>>`，`db.inner().clone()` 得 `Arc<CatalogDB>`，正好匹配新签名。若原来传 `*db.inner()` 或其他，调整为 `db.inner().clone()`。

- [ ] **Step 3: cargo check 验证**

```bash
cd src-tauri && set "PATH=D:\env\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin;D:\env\rust\cargo\bin;%PATH%" && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error 输出（或仅与未完成的 clean/enrich 相关，本任务范围外）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/scan.rs
git commit -m "refactor(scan): start_scan_headless 改 Arc<CatalogDB> 不消费 db"
```

---

### Task 4: 新建 start_clean_headless

**Files:**
- Modify: `src-tauri/src/commands/clean.rs`

- [ ] **Step 1: 读取 start_clean command 完整逻辑**

Read `src-tauri/src/commands/clean.rs` 全文，理解 start_clean 的 spawn 内核心逻辑：
- 解析/生成 actions（自动模式从 db 查，手动模式解析 file_actions）
- 执行清理（dry_run 判断、文件操作）
- 事件广播（clean_error / clean_complete / 进度）

记录所有 `app.emit("xxx", payload)` 调用的事件名与 payload 结构。

- [ ] **Step 2: 添加 start_clean_headless 函数**

在 clean.rs 添加（在 start_clean command 函数之后）：

```rust
use std::sync::Arc;
use tokio::sync::broadcast;

/// headless 版清理：不依赖 Tauri State/AppHandle，通过 event_tx 广播事件。
/// 事件为 JSON 字符串，格式 {"event":"clean_complete|clean_error|clean_progress", "data": ...}
pub async fn start_clean_headless(
    db: Arc<CatalogDB>,
    config: Config,
    confirm: bool,
    file_actions: Vec<serde_json::Value>,
    event_tx: broadcast::Sender<String>,
) -> Result<(), String> {
    let dry_run = !confirm;

    // ── 解析或自动生成操作列表 ──
    let actions = if file_actions.is_empty() {
        // 自动模式：从 DB 获取文件 → 去重 → 生成操作
        // [复制 start_clean command 内的对应逻辑，app.emit 改为 event_tx.send]
        match db.get_file_records("", "active", "", 1, 100_000) {
            Ok((records, _)) => generate_auto_actions(&records, &config),
            Err(e) => {
                let _ = event_tx.send(serde_json::json!({
                    "event": "clean_error",
                    "data": format!("查询文件失败: {}", e)
                }).to_string());
                return Err(e);
            }
        }
    } else {
        // 手动模式：解析 file_actions
        // [复制 start_clean command 内的解析逻辑]
        file_actions.iter().filter_map(|fa| {
            // 解析逻辑...
            None // 替换为实际解析
        }).collect::<Vec<_>>()
    };

    // ── 执行清理 ──
    // [复制 start_clean command 的执行逻辑]
    // 所有 app.emit("xxx", payload) 改为：
    //   let _ = event_tx.send(serde_json::json!({"event":"xxx","data":payload}).to_string());

    let _ = event_tx.send(serde_json::json!({
        "event": "clean_complete",
        "data": {"dry_run": dry_run, "count": 0}
    }).to_string());

    Ok(())
}
```

> **实现指引**：上述是骨架。实现时逐行对照 start_clean command 的 spawn 内逻辑，把每个 `app.emit(EVENT, PAYLOAD)` 替换为 `event_tx.send(json!({"event":EVENT,"data":PAYLOAD}).to_string())`。保留所有业务逻辑（generate_auto_actions、文件操作、log 记录）不变。

- [ ] **Step 3: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error（clean_headless 未被调用也无妨，编译通过即可）。

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/commands/clean.rs
git commit -m "feat(clean): 新建 start_clean_headless（broadcast 事件，无 Tauri 依赖）"
```

---

### Task 5: 新建 start_enrich_headless + get_enrich_status_headless

**Files:**
- Modify: `src-tauri/src/commands/enrich.rs`

- [ ] **Step 1: 读取 start_enrich + get_enrich_status command 逻辑**

Read `src-tauri/src/commands/enrich.rs`，记录：
- start_enrich 的 spawn 内逻辑（AI 补全器创建、逐文件丰富、进度/完成/错误事件）
- get_enrich_status 如何读 enrich_state
- EnrichState 结构体字段（running/processed/total 等）

- [ ] **Step 2: 添加 get_enrich_status_headless**

在 enrich.rs 添加：

```rust
/// headless 版状态查询
pub fn get_enrich_status_headless(
    enrich_state: &Arc<parking_lot::Mutex<EnrichState>>,
) -> serde_json::Value {
    let state = enrich_state.lock();
    serde_json::json!({
        "running": state.running,
        "processed": state.processed,
        "total": state.total,
        // 其他 EnrichState 字段按实际补充
    })
}
```

> 实现时对照 get_enrich_status command 返回的字段，确保一致。

- [ ] **Step 3: 添加 start_enrich_headless**

```rust
/// headless 版 AI 丰富
pub async fn start_enrich_headless(
    db: Arc<CatalogDB>,
    config: Config,
    enrich_state: Arc<parking_lot::Mutex<EnrichState>>,
    params: EnrichParams,  // 实际参数对照 start_enrich command
    event_tx: broadcast::Sender<String>,
) -> Result<(), String> {
    // [复制 start_enrich command 的 spawn 内逻辑]
    // app.emit(EVENT, PAYLOAD) → event_tx.send(json!({"event":EVENT,"data":PAYLOAD}).to_string())
    // enrich_state 通过参数传入（非 State 注入）
    Ok(())
}
```

> EnrichParams 的实际字段对照 start_enrich command 的业务参数（limit/force 等）。实现时以源码为准。

- [ ] **Step 4: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```

- [ ] **Step 5: 提交**

```bash
git add src-tauri/src/commands/enrich.rs
git commit -m "feat(enrich): 新建 start_enrich_headless + get_enrich_status_headless"
```

---

### Task 6: Context 加 enrich_state + ipc 注入

**Files:**
- Modify: `src-tauri/src/app/context.rs`
- Modify: `src-tauri/src/app/ipc.rs`

- [ ] **Step 1: Context 加 enrich_state 字段**

Read `src-tauri/src/app/context.rs`，在 struct 内追加：

```rust
use std::sync::Arc;
use parking_lot::RwLock;

use crate::commands::enrich::EnrichState;
use crate::core::config::Config;
use crate::db::catalog::CatalogDB;

#[derive(Clone)]
pub struct Context {
    pub db: Arc<CatalogDB>,
    pub config: Arc<RwLock<Config>>,
    pub app_handle: tauri::AppHandle,
    pub enrich_state: Arc<parking_lot::Mutex<EnrichState>>,  // 新增
}
```

- [ ] **Step 2: plugin_invoke 注入 enrich_state**

Read `src-tauri/src/app/ipc.rs`，修改 plugin_invoke 函数签名，加 enrich_state 参数，构造 Context 时传入：

```rust
#[tauri::command]
pub async fn plugin_invoke(
    plugin: String,
    action: String,
    args: Option<Value>,
    host: State<'_, Arc<PluginHost>>,
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    enrich_state: State<'_, Arc<parking_lot::Mutex<EnrichState>>>,  // 新增
    app_handle: tauri::AppHandle,
) -> Result<Value, String> {
    let ctx = Context {
        db: db.inner().clone(),
        config: config.inner().clone(),
        app_handle,
        enrich_state: enrich_state.inner().clone(),  // 新增
    };
    host.dispatch(&plugin, &action, args.unwrap_or(Value::Null), &ctx)
        .await
        .map_err(|e| e.to_string())
}
```

ipc.rs 顶部需 import：
```rust
use crate::commands::enrich::EnrichState;
```

- [ ] **Step 3: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/app/context.rs src-tauri/src/app/ipc.rs
git commit -m "feat(app): Context 加 enrich_state，plugin_invoke 注入"
```

---

### Task 7: actions.rs 补齐 7 个 action + broadcast 桥接

**Files:**
- Modify: `src-tauri/src/plugins/filesweep/actions.rs`
- Modify: `src-tauri/src/plugins/filesweep/mod.rs`

- [ ] **Step 1: mod.rs actions() 列表加 3 个新 action**

Read `src-tauri/src/plugins/filesweep/mod.rs`，在 `fn actions()` 的 vec![] 内追加：

```rust
            "files:set_action",
            "files:set_move_target",
            "files:batch_set_action",
```

- [ ] **Step 2: actions.rs 添加 3 个文件操作 action**

Read `src-tauri/src/plugins/filesweep/actions.rs`，在 db_ops action 之后、未实现分支之前添加：

```rust
        // ═════════ files（文件操作预设）═════════
        "files:set_action" => {
            #[derive(serde::Deserialize)]
            struct Args {
                file_id: String,
                action: String,
                #[serde(default)]
                move_target: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            ctx.db.set_file_action(&a.file_id, &a.action, a.move_target.as_deref())?;
            Ok(Value::Null)
        }
        "files:set_move_target" => {
            #[derive(serde::Deserialize)]
            struct Args {
                file_id: String,
                target: String,
            }
            let a: Args = serde_json::from_value(args)?;
            ctx.db.set_file_action(&a.file_id, "", Some(&a.target))?;
            Ok(Value::Null)
        }
        "files:batch_set_action" => {
            #[derive(serde::Deserialize)]
            struct Args {
                file_ids: Vec<String>,
                action: String,
                #[serde(default)]
                move_target: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let count = ctx.db.batch_set_action(&a.file_ids, &a.action, a.move_target.as_deref())?;
            Ok(serde_json::json!({ "updated": count }))
        }
```

- [ ] **Step 3: actions.rs 替换 scan:start 实现（broadcast 桥接）**

找到 `"scan:start" | "clean:start" | "enrich:start" | "enrich:status"` 的 NotImplemented 分支，替换为各 action 的真实实现。

scan:start：
```rust
        "scan:start" => {
            #[derive(serde::Deserialize)]
            struct Args {
                dirs: Vec<String>,
                #[serde(default = "default_true")] recursive: bool,
                #[serde(default)] exclude_dirs: Vec<String>,
                #[serde(default)] exclude_names: Vec<String>,
                #[serde(default)] exclude_exts: Vec<String>,
                #[serde(default = "default_true")] detect_app_dirs: bool,
            }
            let a: Args = serde_json::from_value(args)?;
            let (tx, mut rx) = tokio::sync::broadcast::channel::<String>(16);
            let app = ctx.app_handle.clone();
            // broadcast → app.emit 桥接
            tokio::spawn(async move {
                while let Ok(ev) = rx.recv().await {
                    // headless 发送的 JSON 含 event 字段，解析后用原事件名 emit
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&ev) {
                        if let (Some(event), data) = (parsed.get("event").and_then(|v| v.as_str()), parsed.get("data")) {
                            let _ = app.emit(event, data.cloned());
                        }
                    }
                }
            });
            let config = ctx.config.read().clone();
            commands::scan::start_scan_headless(
                ctx.db.clone(),
                Arc::new(config),
                a.dirs,
                a.recursive,
                a.exclude_dirs,
                a.exclude_names,
                a.exclude_exts,
                a.detect_app_dirs,
                tx,
            ).await?;
            Ok(Value::Null)
        }
```

> 注意：start_scan_headless 当前的事件发送方式（直接 emit 还是 broadcast JSON）。需确认 start_scan_headless 内部是直接用 event_tx.send(String) 还是别的。若它发送的不是 {"event":...,"data":...} 格式，桥接层的解析逻辑需对应调整。实现时 Read start_scan_headless 函数体确认其 event_tx.send 的 payload 格式。

- [ ] **Step 4: actions.rs 添加 clean:start 实现**

```rust
        "clean:start" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)] confirm: bool,
                #[serde(default)] file_actions: Vec<Value>,
            }
            let a: Args = serde_json::from_value(args)?;
            let (tx, mut rx) = tokio::sync::broadcast::channel::<String>(16);
            let app = ctx.app_handle.clone();
            tokio::spawn(async move {
                while let Ok(ev) = rx.recv().await {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&ev) {
                        if let (Some(event), data) = (parsed.get("event").and_then(|v| v.as_str()), parsed.get("data")) {
                            let _ = app.emit(event, data.cloned());
                        }
                    }
                }
            });
            let config = ctx.config.read().clone();
            commands::clean::start_clean_headless(
                ctx.db.clone(), config, a.confirm, a.file_actions, tx,
            ).await?;
            Ok(Value::Null)
        }
```

- [ ] **Step 5: actions.rs 添加 enrich:start + enrich:status 实现**

```rust
        "enrich:start" => {
            // params 对照 start_enrich command 的业务参数
            let (tx, mut rx) = tokio::sync::broadcast::channel::<String>(16);
            let app = ctx.app_handle.clone();
            tokio::spawn(async move {
                while let Ok(ev) = rx.recv().await {
                    if let Ok(parsed) = serde_json::from_str::<Value>(&ev) {
                        if let (Some(event), data) = (parsed.get("event").and_then(|v| v.as_str()), parsed.get("data")) {
                            let _ = app.emit(event, data.cloned());
                        }
                    }
                }
            });
            let config = ctx.config.read().clone();
            commands::enrich::start_enrich_headless(
                ctx.db.clone(), config, ctx.enrich_state.clone(), args, tx,
            ).await?;
            Ok(Value::Null)
        }
        "enrich:status" => {
            Ok(commands::enrich::get_enrich_status_headless(&ctx.enrich_state))
        }
```

- [ ] **Step 6: 移除 NotImplemented 分支**

删除剩余的 `_ => Err(PluginError::UnknownAction(...))` 之前的 NotImplemented 分支（若所有 action 已实现）。保留 `_ =>` 作为未知 action 兜底。

- [ ] **Step 7: cargo check**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error"
```
Expected: 无 error。若有签名不匹配（如 start_clean_headless 参数顺序），按报错调整 actions.rs 调用。

- [ ] **Step 8: 提交**

```bash
git add src-tauri/src/plugins/filesweep/
git commit -m "feat(plugins): 补齐 7 个 action（files/scan:start/clean:start/enrich）+ broadcast 桥接"
```

---

### 📋 阶段 A 检查点

- [ ] **Step 1: cargo check 全量**

```bash
cd src-tauri && cargo check 2>&1 | findstr /i "error\|warning: unused"
```
Expected: 无 error。

- [ ] **Step 2: 用户运行时验证（可选，可与阶段 B 后一起）**

DevTools:
```js
await window.__TAURI__.core.invoke("plugin_invoke", {plugin:"filesweep", action:"files:set_action", args:{file_id:"test", action:"delete"}})
await window.__TAURI__.core.invoke("plugin_invoke", {plugin:"filesweep", action:"enrich:status", args:{}})
```

---

# 阶段 B：前端迁移

### Task 8: stores 全量迁移 invoke→pluginInvoke

**Files:**
- Modify: `src/plugins/filesweep/stores/catalog.ts`
- Modify: `src/plugins/filesweep/stores/files.ts`
- Modify: `src/plugins/filesweep/stores/settings.ts`

- [ ] **Step 1: catalog.ts 迁移**

Read `src/plugins/filesweep/stores/catalog.ts`。

顶部 import 区，将 `import { invoke } from "@/lib/api"` 改为 `import { pluginInvoke } from "@/lib/pluginInvoke"`（若还用了其他 api 导出如 listen，保留 api import 并追加 pluginInvoke import）。

替换 2 处：
- `await invoke("update_catalog_entry", { id, ...data })` → `await pluginInvoke("filesweep", "catalog:update", { id, ...data })`
- `await invoke("delete_catalog_entry", { id })` → `await pluginInvoke("filesweep", "catalog:delete", { id })`

> 注意参数结构：pluginInvoke 第 3 参数是 args 对象。原 invoke 的第 2 参数整体作为 args。

- [ ] **Step 2: files.ts 迁移**

Read `src/plugins/filesweep/stores/files.ts`。import 同 Step 1。

替换 5 处：
- `await invoke("start_scan", { dirs, recursive, ... })` → `await pluginInvoke("filesweep", "scan:start", { dirs, recursive, ... })`
- `await invoke("set_file_action", { fileId, action, moveTarget })` → `await pluginInvoke("filesweep", "files:set_action", { file_id: fileId, action, move_target: moveTarget })`
- `await invoke("set_move_target", { fileId, target })` → `await pluginInvoke("filesweep", "files:set_move_target", { file_id: fileId, target })`
- `await invoke("batch_set_action", { fileIds, action, moveTarget })` → `await pluginInvoke("filesweep", "files:batch_set_action", { file_ids: fileIds, action, move_target: moveTarget })`
- `await invoke("start_clean", { confirm, fileActions })` → `await pluginInvoke("filesweep", "clean:start", { confirm, file_actions: fileActions })`

> **重要**：参数名从 camelCase 改为 snake_case（后端 Rust 约定）。fileId→file_id, moveTarget→move_target, fileIds→file_ids, fileActions→file_actions。

- [ ] **Step 3: settings.ts 迁移**

Read `src/plugins/filesweep/stores/settings.ts`。

替换 3 处：
- `await invoke("update_settings", data)` → `await pluginInvoke("filesweep", "settings:update", data)`
- `await invoke("update_settings", { rules: config.value.rules })` → `await pluginInvoke("filesweep", "settings:update", { rules: config.value.rules })`
- `await invoke("reset_db")` → `await pluginInvoke("filesweep", "db:reset")`

- [ ] **Step 4: npm run build 验证**

```bash
npm run build
```
Expected: vue-tsc 通过，无类型错误。

- [ ] **Step 5: grep 确认无旧 invoke 残留**

```bash
findstr /s /c:"invoke(" src\plugins\filesweep\stores\*.ts
```
Expected: 无输出（或仅 listen 相关，非 invoke 调用）。

- [ ] **Step 6: 提交**

```bash
git add src/plugins/filesweep/stores/
git commit -m "refactor(stores): invoke→pluginInvoke 全量迁移（9 处）"
```

---

### Task 9: nav badge 通用化

**Files:**
- Modify: `src/plugins/filesweep/nav.ts`
- Modify: `src/shell/Sidebar.vue`
- Modify: `src/shell/AppShell.vue`

- [ ] **Step 1: nav.ts 给重复文件项加 badge**

Read `src/plugins/filesweep/nav.ts`。badge 函数需要访问 filesStore——但 nav.ts 是静态数据，不能直接用 store。

决策：nav.ts 的 badge 设为一个**标识字符串**（如 `"duplicates"`），Sidebar/AppShell 根据标识查实际值。

修改「重复文件」item：
```ts
{ label: "重复文件", icon: "Copy", route: "/files", query: { dup: "1" }, badge: () => "duplicates" as any },
```

> 这是个简化方案。更干净的方式是 NavItem.badge 返回函数，但 nav.ts 静态定义无法持有 store 引用。P2 用标识字符串，AppShell 映射。

实际更优：去掉 nav.ts 的 badge，改为 AppShell 通过 plugin id + item label 注入 badge 值。Sidebar 接受 `badges: Record<string, number|string|undefined>` prop（key = `${pluginId}:${itemLabel}`）。

采用此方案：nav.ts 不加 badge（保持静态），badge 完全由 AppShell 注入。

- [ ] **Step 2: Sidebar.vue 改用 badges prop**

Read `src/shell/Sidebar.vue`。

替换 `:duplicate-count="filesStore.stats.duplicates"` 机制为通用 badges prop：

```ts
const props = defineProps<{
  categoryNav?: { title: string; items: { label: string; route: string; query: Record<string, string> }[] };
  /** badge 数据，key = "${pluginId}:${itemLabel}" */
  badges?: Record<string, number | string | undefined>;
}>();
```

template 内 badge 显示改为：
```vue
<Badge
  v-if="badges && badges[`${plugin.id}:${item.label}`]"
  variant="secondary"
  class="ml-auto text-[10px] px-1"
>
  {{ badges[`${plugin.id}:${item.label}`] }}
</Badge>
```

> Sidebar 需知道 pluginId——遍历 allGroups 时记录来源 plugin。调整 allGroups 计算为带 pluginId 的结构。

- [ ] **Step 3: AppShell.vue 传 badges**

Read `src/shell/AppShell.vue`。

添加 badges computed：
```ts
const badges = computed(() => ({
  "filesweep:重复文件": filesStore.stats.duplicates || undefined,
}));
```

Sidebar 用法改为：
```vue
<Sidebar
  :class="sidebarCollapsed ? 'w-0 overflow-hidden' : 'w-[200px]'"
  :category-nav="categoryNav"
  :badges="badges"
/>
```

- [ ] **Step 4: npm run build**

```bash
npm run build
```

- [ ] **Step 5: 提交**

```bash
git add src/plugins/filesweep/nav.ts src/shell/Sidebar.vue src/shell/AppShell.vue
git commit -m "feat(shell): nav badge 通用化（badges prop 注入）"
```

---

### 📋 阶段 B 检查点

- [ ] **Step 1: npm run build 通过**

- [ ] **Step 2: grep 确认 stores 无旧 invoke**

```bash
findstr /s /c:"invoke(" src\plugins\filesweep\stores\*.ts 2>nul
```
Expected: 无输出。

- [ ] **Step 3: 用户运行时全功能走查（npm run tauri dev）**

1. 扫描：触发 scan:start，进度事件正常，完成显示结果
2. 文件操作：FileListView 选文件设 action（delete/move），设 move_target
3. 批量操作：选中多文件批量设 action
4. 清理：executeCleanup 执行，clean_complete 事件触发刷新
5. AI 丰富：start_enrich 执行，进度显示，完成
6. 设置：更新规则/设置生效
7. 重置 DB：reset_db 清空
8. 侧栏「重复文件」badge 显示计数

---

## 验收标准（对齐设计文档第 6 节）

- [ ] cargo check 通过
- [ ] npm run build 通过
- [ ] DB migration 应用后 file_records 含 action/move_target 列
- [ ] plugin_invoke files:set_action 可设置文件 action
- [ ] plugin_invoke scan:start 触发扫描并广播事件
- [ ] plugin_invoke clean:start 执行清理
- [ ] plugin_invoke enrich:start 启动 AI 丰富
- [ ] stores grep 无旧 invoke 残留
- [ ] FileListView 批量操作/单文件 action 可用
- [ ] 侧栏重复文件 badge 显示

---

## 已知风险点（实现时关注）

1. **start_scan_headless 的事件格式**：Task 7 Step 3 桥接层假设 event_tx.send 的是 `{"event":...,"data":...}` JSON。但 start_scan_headless 可能直接发送原始字符串（如 `"scan_progress"`）。**实现 Task 7 前必须 Read start_scan_headless 函数体确认其 send 格式**，桥接层解析逻辑对应调整。最稳妥：让所有 headless 函数统一发送 `{"event":EVENT,"data":DATA}` 格式（Task 4/5 新建的 clean/enrich headless 按此格式；scan headless 若格式不同则在其函数内统一或桥接层特判）。

2. **参数 camelCase→snake_case**：Task 8 stores 迁移时，前端参数名必须改 snake_case（fileId→file_id）。漏改会导致后端反序列化失败（字段缺失用 default，逻辑错误）。

3. **enrich_state 类型**：Context 持有 `Arc<parking_lot::Mutex<EnrichState>>`。确认 lib.rs manage 的 enrich_state 就是此类型（P1 已 manage enrich_state，类型应一致）。

4. **start_clean/enrich 的 file_actions/params 解析**：Task 4/5 提取 headless 时，原 command 的参数解析逻辑需完整搬入。file_actions 是 `Vec<Value>`，解析逻辑可能复杂，仔细对照原码。
