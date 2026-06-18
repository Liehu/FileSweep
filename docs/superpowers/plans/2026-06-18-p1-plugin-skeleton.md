# P1 插件骨架 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 建立 Tauri 插件化内核（后端 Plugin trait + 分发器，前端 definePlugin + 动态路由 + 命令面板），并将现有 filesweep 功能适配为首个内置插件。

**Architecture:** 后端新增 `app/` 宿主内核（Plugin trait + PluginHost + plugin_invoke 分发命令）与 `plugins/filesweep/` 适配层；现有 `commands/` 的核心逻辑提取为接受普通参数的内部函数（解除对 Tauri `State` 注入的耦合），command 函数保留为薄壳。前端新增 `lib/plugin.ts` 契约、`shell/` 宿主组件、`plugins/filesweep/` 子目录，现有 views/stores/router 迁入。双轨并存：现有 26 命令保留可用。

**Tech Stack:** Rust + Tauri v2 + async-trait + thiserror；Vue3 + TypeScript + vue-router + radix-vue + lucide-vue-next

**关联设计文档：** `docs/superpowers/specs/2026-06-18-p1-plugin-skeleton-design.md`

**关键事实（已核实）：**
- 现有 26 个 `#[tauri::command]` 函数大量依赖 `State<'_, T>` / `AppHandle` 注入参数，无法被普通函数直接调用
- 解耦策略：把 command 函数核心逻辑提取为 `pub async fn xxx_impl(db: Arc<...>, config: ..., ...) -> Result<...>`，command 函数变为薄壳调用 `_impl`
- `start_scan` 等已有 `let db = db.inner().clone();` 模式，提取成本低
- 本机无 Rust 工具链（cargo 不可用）—— **P1 后端代码写完后无法在本机编译验证**，需用户在有 Rust 环境的机器跑 `cargo check`。前端 `npm run build` 可验证。
- 现有前端 `src/` 通过 `@` alias 引用自身；迁移后加 `@plugins` alias

**验证约束：** 因无 cargo，每个后端任务的「验证」步骤改为：(a) Rust 语法自检（人工 review 签名一致性）；(b) 待用户在 Rust 环境统一跑 `cargo check`。前端任务可用 `npm run build` 实时验证。

---

## 文件结构

**新增（后端）：**
- `src-tauri/src/app/mod.rs` — 宿主内核模块入口
- `src-tauri/src/app/plugin.rs` — Plugin trait + PluginMetadata + PluginFeature + FeatureType + PluginType + PluginPermissions + PluginError
- `src-tauri/src/app/context.rs` — Context（db/config/app_handle 共享状态）
- `src-tauri/src/app/host.rs` — PluginHost 注册表 + dispatch + start_system_plugins
- `src-tauri/src/app/ipc.rs` — plugin_invoke / plugin_list Tauri 命令
- `src-tauri/src/plugins/mod.rs` — register_all 汇总
- `src-tauri/src/plugins/filesweep/mod.rs` — FileSweepPlugin 实现
- `src-tauri/src/plugins/filesweep/actions.rs` — action → _impl 转发

**修改（后端）：**
- `src-tauri/src/lib.rs` — 注册 PluginHost + 2 新命令 + global-shortcut 插件
- `src-tauri/src/main.rs` — 无改动（headless 不适配）
- `src-tauri/src/commands/*.rs` — 8 个文件的 command 函数提取 `_impl`（按需，涉及 State 注入的）
- `src-tauri/Cargo.toml` — 新增 async-trait / thiserror / tauri-plugin-global-shortcut
- `src-tauri/capabilities/default.json` — global-shortcut 权限

**新增（前端）：**
- `src/lib/plugin.ts` — definePlugin + PluginManifest + getAllFeatures
- `src/lib/pluginInvoke.ts` — pluginInvoke 封装
- `src/plugins/_registry.ts` — 插件汇总入口
- `src/plugins/filesweep/index.ts` — filesweep manifest
- `src/plugins/filesweep/routes.ts` — 从 router 迁入的路由
- `src/plugins/filesweep/nav.ts` — 从 App.vue 迁入的导航
- `src/shell/AppShell.vue` — 宿主壳（取代 App.vue 主体）
- `src/shell/Sidebar.vue` — 动态侧栏
- `src/shell/CommandPalette.vue` — 命令面板
- `src/shell/iconMap.ts` — lucide 图标名→组件映射

**修改（前端）：**
- `src/router/index.ts` — 改为动态路由组装
- `src/App.vue` — 简化为挂载 AppShell
- `src/main.ts` — 触发插件注册
- `tsconfig.json` — 加 @plugins alias
- `vite.config.ts` — 加 @plugins alias

**移动（前端）：**
- `src/views/*` → `src/plugins/filesweep/views/*`
- `src/stores/*` → `src/plugins/filesweep/stores/*`

---

## 阶段划分

本计划分两阶段，每阶段末有验证检查点：

- **阶段 A（后端骨架）**：Task 1-6。交付可编译的 Rust 插件内核 + filesweep 适配。检查点：用户跑 `cargo check`。
- **阶段 B（前端骨架）**：Task 7-13。交付动态路由 + 宿主组件 + 命令面板。检查点：`npm run build` + 手动走查。

---

# 阶段 A：后端插件骨架

### Task 1: Cargo 依赖与 app 模块骨架

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/app/mod.rs`

- [ ] **Step 1: 添加 Cargo 依赖**

读取 `src-tauri/Cargo.toml`，在 `[dependencies]` 段添加（若已存在则跳过）：

```toml
async-trait = "0.1"
thiserror = "1"
tauri-plugin-global-shortcut = "2"
```

- [ ] **Step 2: 创建 app/mod.rs 模块入口**

Create `src-tauri/src/app/mod.rs`:

```rust
pub mod plugin;
pub mod context;
pub mod host;
pub mod ipc;

pub use plugin::{Plugin, PluginMetadata, PluginFeature, PluginType, FeatureType, PluginPermissions, PluginError};
pub use context::Context;
pub use host::PluginHost;
```

- [ ] **Step 3: 在 lib.rs 声明 app 模块**

Modify `src-tauri/src/lib.rs` 顶部模块声明区，添加：

```rust
pub mod app;
pub mod plugins;
```

（`plugins` 模块在 Task 6 创建，此处先声明会导致编译错误，Task 6 补齐。或本步只加 `pub mod app;`，Task 6 再加 `pub mod plugins;`。采用后者。）

实际本步只添加：
```rust
pub mod app;
```

- [ ] **Step 4: 提交**

```bash
git add src-tauri/Cargo.toml src-tauri/src/app/mod.rs src-tauri/src/lib.rs
git commit -m "feat(app): 初始化 app 宿主内核模块与 Cargo 依赖"
```

---

### Task 2: Plugin trait 与类型定义

**Files:**
- Create: `src-tauri/src/app/plugin.rs`

- [ ] **Step 1: 写入 plugin.rs 完整定义**

Create `src-tauri/src/app/plugin.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::app::context::Context;

/// 插件类型（rubick 启发）
/// - Ui: 有界面，通过 features 关键词触发
/// - System: 无界面，启动时加载（P1 无 system 插件，预留）
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Ui,
    System,
}

/// feature 类型（kunkun 启发预留）
/// - Route: 进入路由（P1 全部此类型）
/// - Template: 宿主渲染表单（P5 扩展）
/// - Action: 纯命令无路由（P5 扩展）
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FeatureType {
    Route,
    Template,
    Action,
}

impl Default for FeatureType {
    fn default() -> Self {
        FeatureType::Route
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginFeature {
    pub code: String,
    pub explain: String,
    pub cmds: Vec<String>,
    #[serde(default)]
    pub feature_type: FeatureType,
    /// Route 类型必填
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
}

/// 插件权限声明（kunkun 启发预留）
/// P1 内置插件默认 All，P5 第三方插件显式声明能力
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    /// ["*"] = 全权限；["fs:read","shell:exec"] = 细粒度
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl PluginPermissions {
    pub fn all() -> Self {
        Self {
            capabilities: vec!["*".to_string()],
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub plugin_type: PluginType,
    pub features: Vec<PluginFeature>,
    pub version: String,
    #[serde(default = "PluginPermissions::all")]
    pub permissions: PluginPermissions,
}

#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("plugin not found: {0}")]
    NotFound(String),
    #[error("unknown action: {0}")]
    UnknownAction(String),
    #[error("{0}")]
    Internal(String),
}

impl From<serde_json::Error> for PluginError {
    fn from(e: serde_json::Error) -> Self {
        PluginError::Internal(e.to_string())
    }
}

/// 插件 trait：所有内置插件实现此接口
#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;

    /// 声明支持的 action（校验/文档用）
    fn actions(&self) -> Vec<&'static str> {
        vec![]
    }

    /// 处理 invoke：action 为命令名（如 "scan:start"），args 为参数
    async fn invoke(&self, action: &str, args: Value, ctx: &Context) -> Result<Value, PluginError>;

    /// system 插件启动钩子（ui 插件默认空实现）
    async fn on_start(&self, _ctx: &Context) -> Result<(), PluginError> {
        Ok(())
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/app/plugin.rs
git commit -m "feat(app): 定义 Plugin trait 与元数据类型"
```

> 注：`plugin.rs` 引用 `Context`，需 Task 3 的 `context.rs` 存在才能编译。本任务与 Task 3 一并提交也可。为减少编译错误窗口，建议 Task 2 和 Task 3 连续完成后统一 cargo check。

---

### Task 3: Context 共享状态

**Files:**
- Create: `src-tauri/src/app/context.rs`

- [ ] **Step 1: 写入 context.rs**

Create `src-tauri/src/app/context.rs`:

```rust
use std::sync::Arc;
use parking_lot::RwLock;

use crate::core::config::Config;
use crate::db::catalog::CatalogDB;

/// 插件运行时上下文：提供对共享状态的访问
///
/// 复用 lib.rs 已 manage 的 db / config，插件无需自行初始化。
#[derive(Clone)]
pub struct Context {
    pub db: Arc<CatalogDB>,
    pub config: Arc<RwLock<Config>>,
    pub app_handle: tauri::AppHandle,
}
```

- [ ] **Step 2: 人工核对类型一致性**

确认 `lib.rs` 中 manage 的类型：
- `db: CatalogDB` 被 `.manage(db)` —— 但 Context 要 `Arc<CatalogDB>`。需检查 `CatalogDB::open` 返回的是 `CatalogDB` 还是 `Arc<CatalogDB>`。

读取 `src-tauri/src/lib.rs` 的 db 初始化段，确认类型。若 db 非 Arc，则需在 lib.rs 用 `Arc::new(db)` 包装后 manage，或 Context 直接持有 `CatalogDB`（clone 成本评估）。

若 lib.rs 当前是 `.manage(db)` 且 db 为 `CatalogDB`（非 Arc），则：
- 方案：lib.rs 改为 `let db = Arc::new(db); .manage(db.clone())`，Context 持有 `Arc<CatalogDB>` 一致
- 记录此调整为 Task 5 lib.rs 改造的一部分

- [ ] **Step 3: 提交**

```bash
git add src-tauri/src/app/context.rs
git commit -m "feat(app): 定义 Context 共享状态"
```

---

### Task 4: PluginHost 注册表与分发

**Files:**
- Create: `src-tauri/src/app/host.rs`

- [ ] **Step 1: 写入 host.rs**

Create `src-tauri/src/app/host.rs`:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::{Plugin, PluginError, PluginMetadata, PluginType};

/// 插件注册表：管理所有已注册插件，提供分发能力
pub struct PluginHost {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.metadata().id.clone();
        log::info!("registering plugin: {}", id);
        self.plugins.insert(id, plugin);
    }

    /// 返回所有插件元信息（plugin_list 命令用）
    pub fn metadata_list(&self) -> Vec<PluginMetadata> {
        self.plugins.values().map(|p| p.metadata().clone()).collect()
    }

    /// 启动所有 system 插件
    pub async fn start_system_plugins(&self, ctx: &Context) -> Result<(), PluginError> {
        for p in self.plugins.values() {
            if p.metadata().plugin_type == PluginType::System {
                log::info!("starting system plugin: {}", p.metadata().id);
                p.on_start(ctx).await?;
            }
        }
        Ok(())
    }

    /// 分发 invoke 到目标插件
    pub async fn dispatch(
        &self,
        plugin_id: &str,
        action: &str,
        args: Value,
        ctx: &Context,
    ) -> Result<Value, PluginError> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
        plugin.invoke(action, args, ctx).await
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/app/host.rs
git commit -m "feat(app): 实现 PluginHost 注册表与分发"
```

---

### Task 5: IPC 命令 plugin_invoke / plugin_list

**Files:**
- Create: `src-tauri/src/app/ipc.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: 写入 ipc.rs**

Create `src-tauri/src/app/ipc.rs`:

```rust
use std::sync::Arc;

use serde_json::Value;
use tauri::State;

use crate::app::context::Context;
use crate::app::host::PluginHost;
use crate::app::plugin::PluginMetadata;
use crate::core::config::Config;
use crate::db::catalog::CatalogDB;

/// 插件统一调用入口
/// 前端：invoke("plugin_invoke", { plugin, action, args })
#[tauri::command]
pub async fn plugin_invoke(
    plugin: String,
    action: String,
    args: Option<Value>,
    host: State<'_, Arc<PluginHost>>,
    db: State<'_, Arc<CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<Config>>>,
    app_handle: tauri::AppHandle,
) -> Result<Value, String> {
    let ctx = Context {
        db: db.inner().clone(),
        config: config.inner().clone(),
        app_handle,
    };
    host.dispatch(&plugin, &action, args.unwrap_or(Value::Null), &ctx)
        .await
        .map_err(|e| e.to_string())
}

/// 列出所有插件元信息（前端命令面板/侧栏渲染用）
#[tauri::command]
pub fn plugin_list(host: State<'_, Arc<PluginHost>>) -> Vec<PluginMetadata> {
    host.metadata_list()
}
```

> 注：`plugin_invoke` 的 `db`/`config` 参数类型是 `State<'_, Arc<...>>`。这要求 lib.rs 中 manage 的就是 `Arc<CatalogDB>` 和 `Arc<RwLock<Config>>`。Task 7 改造 lib.rs 时确保一致。现有 lib.rs manage 的 config 已是 `Arc<RwLock<Config>>`；db 若非 Arc 需调整（见 Task 3 Step 2 记录）。

- [ ] **Step 2: 提交**

```bash
git add src-tauri/src/app/ipc.rs
git commit -m "feat(app): 添加 plugin_invoke/plugin_list IPC 命令"
```

---

### Task 6: filesweep 插件适配层（含 commands _impl 提取）

这是 P1 最复杂的任务。现有 commands 依赖 `State` 注入，需提取核心逻辑为 `_impl` 函数。

**Files:**
- Create: `src-tauri/src/plugins/mod.rs`
- Create: `src-tauri/src/plugins/filesweep/mod.rs`
- Create: `src-tauri/src/plugins/filesweep/actions.rs`
- Modify: `src-tauri/src/commands/scan.rs`（提取 _impl）
- Modify: `src-tauri/src/commands/catalog.rs`（提取 _impl）
- Modify: `src-tauri/src/commands/clean.rs`（提取 _impl）
- Modify: `src-tauri/src/commands/enrich.rs`（提取 _impl）
- Modify: 其他 commands/*.rs（按需）

**策略说明：**
由于 26 个命令逐个提取 `_impl` 工作量大且无法本机编译验证，采用**分批策略**：
- 本 Task 先提取 **scan + catalog** 两个高频模块（覆盖核心扫描/查询/导出），建立 `_impl` 提取模式
- 其余 commands 的 `_impl` 提取作为 Task 6b（可选，P1 可让对应 action 先返回 "not implemented"，P2 补齐）
- **重要**：双轨并存策略下，现有 26 命令仍可直接用，filesweep 插件的未适配 action 不阻塞主功能

- [ ] **Step 1: 提取 scan.rs 的 _impl 函数**

读取 `src-tauri/src/commands/scan.rs` 完整内容。对每个 `#[tauri::command] pub async fn xxx(...)` 函数：

1. 复制函数体，创建 `pub async fn xxx_impl(db: Arc<CatalogDB>, config: ..., <业务参数>) -> Result<返回类型, String>`，函数体不变，只是把 `db.inner().clone()` 这类提取去掉（直接用传入的 db）
2. 原 `#[tauri::command]` 函数改为薄壳：提取 State 后调用 `_impl`

示例（`get_file_stats`，签名简单）：

原：
```rust
#[tauri::command]
pub async fn get_file_stats(db: State<'_, CatalogDB>) -> Result<FileStats, String> {
    db.get_file_stats()
}
```

改为：
```rust
pub async fn get_file_stats_impl(db: &CatalogDB) -> Result<FileStats, String> {
    db.get_file_stats()
}

#[tauri::command]
pub async fn get_file_stats(db: State<'_, CatalogDB>) -> Result<FileStats, String> {
    get_file_stats_impl(db.inner()).await
}
```

对 `start_scan`（复杂，含 AppHandle + spawn）：`_impl` 接受 `app: AppHandle, db: Arc<CatalogDB>, config: Config, <参数>`，函数体（含 tokio::spawn）原样移入。原 command 函数提取 `db.inner().clone()` 和 `config.inner().read().clone()` 后调用 `_impl`。

- [ ] **Step 2: 提取 catalog.rs 的 _impl 函数**

同样模式处理 `get_catalog` / `update_catalog_entry` / `delete_catalog_entry` / `export_catalog`。

- [ ] **Step 3: 创建 plugins/mod.rs**

Create `src-tauri/src/plugins/mod.rs`:

```rust
pub mod filesweep;

pub use filesweep::FileSweepPlugin;
```

- [ ] **Step 4: 创建 plugins/filesweep/mod.rs**

Create `src-tauri/src/plugins/filesweep/mod.rs`:

```rust
pub mod actions;

use async_trait::async_trait;
use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::{
    FeatureType, Plugin, PluginError, PluginFeature, PluginMetadata, PluginPermissions,
    PluginType,
};

pub struct FileSweepPlugin {
    meta: PluginMetadata,
}

impl FileSweepPlugin {
    pub fn new() -> Self {
        Self {
            meta: PluginMetadata {
                id: "filesweep".into(),
                name: "文件整理".into(),
                icon: "Folder".into(),
                plugin_type: PluginType::Ui,
                features: vec![
                    PluginFeature {
                        code: "files".into(),
                        explain: "全部文件".into(),
                        cmds: vec!["文件".into(), "files".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/files".into()),
                    },
                    PluginFeature {
                        code: "scan".into(),
                        explain: "扫描文件".into(),
                        cmds: vec!["扫描".into(), "scan".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/scan".into()),
                    },
                    PluginFeature {
                        code: "dedup".into(),
                        explain: "重复文件".into(),
                        cmds: vec!["去重".into(), "重复".into(), "dedup".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/files".into()),
                    },
                    PluginFeature {
                        code: "catalog".into(),
                        explain: "软件目录".into(),
                        cmds: vec!["目录".into(), "catalog".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/catalog".into()),
                    },
                    PluginFeature {
                        code: "enrich".into(),
                        explain: "AI 丰富".into(),
                        cmds: vec!["AI".into(), "丰富".into(), "enrich".into()],
                        feature_type: FeatureType::Route,
                        route: Some("/enrich".into()),
                    },
                ],
                version: env!("CARGO_PKG_VERSION").into(),
                permissions: PluginPermissions::all(),
            },
        }
    }
}

impl Default for FileSweepPlugin {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Plugin for FileSweepPlugin {
    fn metadata(&self) -> &PluginMetadata {
        &self.meta
    }

    fn actions(&self) -> Vec<&'static str> {
        vec![
            "scan:start",
            "scan:files",
            "scan:stats",
            "scan:suggestions",
            "clean:start",
            "catalog:get",
            "catalog:update",
            "catalog:delete",
            "catalog:export",
            "enrich:start",
            "enrich:status",
            "settings:get",
            "settings:update",
            "rules:get",
            "rules:update",
            "categories:get",
            "categories:update",
            "tags:get",
            "tags:create",
            "tags:update",
            "tags:delete",
            "logs:get",
            "logs:revert",
            "logs:batch_revert",
            "db:reset",
        ]
    }

    async fn invoke(
        &self,
        action: &str,
        args: Value,
        ctx: &Context,
    ) -> Result<Value, PluginError> {
        actions::dispatch(action, args, ctx).await
    }
}
```

- [ ] **Step 5: 创建 plugins/filesweep/actions.rs**

Create `src-tauri/src/plugins/filesweep/actions.rs`。本 Task 先实现 scan + catalog 的 action（其余返回 NotImplemented，P2 补齐）：

```rust
use serde_json::Value;

use crate::app::context::Context;
use crate::app::plugin::PluginError;
use crate::commands;

/// filesweep 插件 action 分发
pub async fn dispatch(action: &str, args: Value, ctx: &Context) -> Result<Value, PluginError> {
    match action {
        // === scan ===
        "scan:stats" => {
            let stats = commands::scan::get_file_stats_impl(&ctx.db).await?;
            Ok(serde_json::to_value(stats)?)
        }
        "scan:files" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)]
                page: Option<i32>,
                #[serde(default)]
                page_size: Option<i32>,
                #[serde(default)]
                category: Option<String>,
                #[serde(default)]
                status: Option<String>,
                #[serde(default)]
                keyword: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let result = commands::scan::get_files_impl(
                &ctx.db,
                a.page,
                a.page_size,
                a.category,
                a.status,
                a.keyword,
            )
            .await?;
            Ok(serde_json::to_value(result)?)
        }
        "scan:start" => {
            #[derive(serde::Deserialize)]
            struct Args {
                dirs: Vec<String>,
                #[serde(default = "default_true")]
                recursive: bool,
                #[serde(default)]
                exclude_dirs: Vec<String>,
                #[serde(default)]
                exclude_names: Vec<String>,
                #[serde(default)]
                exclude_exts: Vec<String>,
                #[serde(default = "default_true")]
                detect_app_dirs: bool,
            }
            let a: Args = serde_json::from_value(args)?;
            let config = ctx.config.read().clone();
            commands::scan::start_scan_impl(
                ctx.app_handle.clone(),
                ctx.db.clone(),
                config,
                a.dirs,
                a.recursive,
                a.exclude_dirs,
                a.exclude_names,
                a.exclude_exts,
                a.detect_app_dirs,
            )
            .await?;
            Ok(Value::Null)
        }
        "scan:suggestions" => {
            let config = ctx.config.read().clone();
            let result =
                commands::scan::get_suggestions_impl(ctx.app_handle.clone(), ctx.db.clone(), config)
                    .await?;
            Ok(serde_json::to_value(result)?)
        }

        // === catalog ===
        "catalog:get" => {
            #[derive(serde::Deserialize)]
            struct Args {
                #[serde(default)]
                page: Option<i32>,
                #[serde(default)]
                page_size: Option<i32>,
                #[serde(default)]
                keyword: Option<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let result =
                commands::catalog::get_catalog_impl(&ctx.db, a.page, a.page_size, a.keyword).await?;
            Ok(serde_json::to_value(result)?)
        }
        "catalog:export" => {
            #[derive(serde::Deserialize)]
            struct Args {
                format: String,
            }
            let a: Args = serde_json::from_value(args)?;
            let result = commands::catalog::export_catalog_impl(&ctx.db, &a.format).await?;
            Ok(Value::String(result))
        }
        "catalog:update" => {
            let result = commands::catalog::update_catalog_entry_impl(&ctx.db, args).await?;
            Ok(serde_json::to_value(result)?)
        }
        "catalog:delete" => {
            #[derive(serde::Deserialize)]
            struct Args {
                ids: Vec<String>,
            }
            let a: Args = serde_json::from_value(args)?;
            let result = commands::catalog::delete_catalog_entry_impl(&ctx.db, &a.ids).await?;
            Ok(serde_json::to_value(result)?)
        }

        // === 其余 action：P2 补齐，P1 暂返回 NotImplemented ===
        _ => Err(PluginError::Internal(format!(
            "action '{}' not yet implemented in filesweep plugin (P2 will add). \
             Use legacy invoke() command directly in the meantime.",
            action
        ))),
    }
}

fn default_true() -> bool {
    true
}
```

> **重要**：`get_files_impl` / `get_catalog_impl` 等函数签名必须与 Task 6 Step 1-2 提取的 `_impl` 签名**完全一致**。实现时以实际提取的签名为准调整本文件的调用参数。`update_catalog_entry_impl` 接受 `Value` 是占位，实际签名以提取结果为准（可能是具体结构体）。

- [ ] **Step 6: 提交**

```bash
git add src-tauri/src/plugins/ src-tauri/src/commands/scan.rs src-tauri/src/commands/catalog.rs
git commit -m "feat(plugins): filesweep 插件适配层（scan/catalog action）

- 提取 scan.rs/catalog.rs 的 _impl 函数解除 State 注入耦合
- 实现 scan:stats/files/start/suggestions、catalog:get/export/update/delete action
- 其余 action P2 补齐（双轨并存：现有命令仍可直接用）"
```

---

### Task 7: lib.rs 整合 + global-shortcut

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/capabilities/default.json`

- [ ] **Step 1: 读取当前 lib.rs 全文**

读取确认 db 初始化方式（Arc 与否）、manage 调用、invoke_handler 列表。

- [ ] **Step 2: 修改 lib.rs**

在现有 `run()` 函数中：

1. **db 改 Arc 包装**（若当前非 Arc）：
```rust
let db = db::catalog::CatalogDB::open(&config.read().db_path).expect("无法打开数据库");
db.seed_default_tags().ok();
let db = Arc::new(db);   // 新增：Arc 包装
```

2. **构建 PluginHost**（在 Builder 之前）：
```rust
let mut plugin_host = app::PluginHost::new();
plugin_host.register(Box::new(plugins::FileSweepPlugin::new()));
let plugin_host = Arc::new(plugin_host);
```

3. **Builder 链修改**：
```rust
tauri::Builder::default()
    .plugin(tauri_plugin_shell::init())
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_fs::init())
    .plugin(tauri_plugin_global_shortcut::Builder::new().build())   // 新增
    .manage(db.clone())          // 改为 db.clone()（Arc）
    .manage(config.clone())      // 已是 Arc<RwLock<Config>>，clone
    .manage(enrich_state)
    .manage(plugin_host.clone()) // 新增
    .invoke_handler(tauri::generate_handler![
        // === 现有 26 命令保留 ===
        commands::scan::start_scan,
        commands::scan::get_files,
        commands::scan::get_file_stats,
        commands::scan::get_suggestions,
        commands::clean::start_clean,
        commands::catalog::get_catalog,
        commands::catalog::update_catalog_entry,
        commands::catalog::delete_catalog_entry,
        commands::catalog::export_catalog,
        commands::enrich::start_enrich,
        commands::enrich::get_enrich_status,
        commands::settings::get_settings,
        commands::settings::update_settings,
        commands::rules::get_rules,
        commands::rules::update_rules,
        commands::categories::get_func_categories,
        commands::categories::update_func_categories,
        commands::tags::get_tags,
        commands::tags::create_tag,
        commands::tags::update_tag,
        commands::tags::delete_tag,
        commands::logs::get_logs,
        commands::logs::revert_operation,
        commands::logs::batch_revert,
        commands::db_ops::reset_db,
        // === 新增插件分发命令 ===
        app::ipc::plugin_invoke,
        app::ipc::plugin_list,
    ])
    .run(tauri::generate_context!())
    .expect("error while running FileSweep");
```

> 注意：db 改 Arc 后，所有现有 `State<'_, CatalogDB>` 的 command 签名要改为 `State<'_, Arc<CatalogDB>>`，内部 `db.inner()` 改 `db.inner().as_ref()` 或 `&**db.inner()`。这是 Task 6 _impl 提取时一并处理的（command 薄壳调用 _impl 时解引用）。

4. **顶部模块声明补全**（Task 1 只加了 `pub mod app;`）：
```rust
pub mod app;
pub mod plugins;   // 新增
pub mod core;
pub mod db;
pub mod ai;
pub mod commands;
pub mod headless;
```

- [ ] **Step 3: 更新 capabilities/default.json**

读取 `src-tauri/capabilities/default.json`，在 `permissions` 数组添加 global-shortcut 权限：

```json
{
  "permissions": [
    "core:default",
    "shell:allow-open",
    "dialog:default",
    "fs:default",
    "global-shortcut:allow-register",
    "global-shortcut:allow-unregister"
  ]
}
```

（保留现有权限项，追加 global-shortcut 两项）

- [ ] **Step 4: 提交**

```bash
git add src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "feat(app): 整合 PluginHost 到 lib.rs + global-shortcut 权限"
```

---

### 📋 阶段 A 检查点

- [ ] **Step 1: 用户在 Rust 环境运行 cargo check**

```bash
cd src-tauri && cargo check
```

预期：Finished，无 error（warning 可接受，如未使用代码）。

若报错：
- `State<'_, CatalogDB>` vs `State<'_, Arc<CatalogDB>>` 不匹配 → 检查 Task 7 db Arc 包装是否传导到所有 command 签名
- `_impl` 函数签名不匹配 → 以 cargo 报错为准调整 actions.rs 调用
- `cannot find type Context` → 确认 app/mod.rs 的 pub use

- [ ] **Step 2: 用户手动验证 plugin_list**

在应用中通过浏览器 devtools 或临时前端代码调用：
```js
await window.__TAURI__.core.invoke("plugin_list")
```
预期：返回含 filesweep metadata（5 个 features）的数组。

- [ ] **Step 3: 用户手动验证 plugin_invoke**

```js
await window.__TAURI__.core.invoke("plugin_invoke", {
  plugin: "filesweep", action: "scan:stats", args: {}
})
```
预期：返回文件统计对象（等价于原 `get_file_stats` 命令）。

**阶段 A 完成后才进入阶段 B。**

---

# 阶段 B：前端插件骨架

### Task 8: tsconfig/vite alias + plugin.ts 契约

**Files:**
- Modify: `tsconfig.json`
- Modify: `vite.config.ts`
- Create: `src/lib/plugin.ts`
- Create: `src/lib/pluginInvoke.ts`

- [ ] **Step 1: 添加 @plugins alias**

Modify `tsconfig.json` 的 `compilerOptions.paths`：
```json
{
  "compilerOptions": {
    "paths": {
      "@/*": ["./src/*"],
      "@plugins/*": ["./src/plugins/*"]
    }
  }
}
```

Modify `vite.config.ts` 的 `resolve.alias`（读取现有内容后追加）：
```ts
resolve: {
  alias: {
    "@": path.resolve(__dirname, "./src"),
    "@plugins": path.resolve(__dirname, "./src/plugins"),
  },
},
```

（确认现有 `@` 配置，在其基础上加 `@plugins`）

- [ ] **Step 2: 创建 plugin.ts**

Create `src/lib/plugin.ts`:

```ts
import type { RouteRecordRaw } from "vue-router";

export type PluginType = "ui" | "system";

/** feature 类型（kunkun 启发预留）
 * - route: 进入路由（P1 全部此类型）
 * - template: 宿主渲染表单（P5 扩展）
 * - action: 纯命令无路由（P5 扩展）
 */
export type FeatureType = "route" | "template" | "action";

export interface PluginFeature {
  code: string;
  explain: string;
  cmds: string[];
  type?: FeatureType;     // 默认 "route"
  route?: string;         // type=route 时必填
}

export interface NavItem {
  label: string;
  icon: string;           // lucide 图标名
  route: string;
  query?: Record<string, string>;
  badge?: () => string | number | undefined;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
}

export interface PluginManifest {
  id: string;
  name: string;
  icon: string;
  pluginType: PluginType;
  features: PluginFeature[];
  navGroups?: NavGroup[];
  routes?: () => Promise<RouteRecordRaw[]>;
  onActivate?: (featureCode?: string) => void;
  /** 权限声明（默认 ["*"] 全权限，P1 内置可信）。P5 第三方插件显式声明 */
  permissions?: string[];
}

const registry = new Map<string, PluginManifest>();

export function definePlugin(m: PluginManifest): PluginManifest {
  if (registry.has(m.id)) {
    throw new Error(`plugin already registered: ${m.id}`);
  }
  registry.set(m.id, m);
  return m;
}

export function getPlugins(): PluginManifest[] {
  return Array.from(registry.values());
}

export function getPlugin(id: string): PluginManifest | undefined {
  return registry.get(id);
}

/** 命令面板搜索用的扁平化 feature */
export interface SearchableFeature {
  code: string;
  explain: string;
  cmds: string[];
  type: FeatureType;
  route?: string;
  pluginId: string;
  pluginName: string;
  pluginIcon: string;
}

export function getAllFeatures(): SearchableFeature[] {
  const result: SearchableFeature[] = [];
  for (const plugin of getPlugins()) {
    for (const feature of plugin.features) {
      result.push({
        code: feature.code,
        explain: feature.explain,
        cmds: feature.cmds,
        type: feature.type ?? "route",
        route: feature.route,
        pluginId: plugin.id,
        pluginName: plugin.name,
        pluginIcon: plugin.icon,
      });
    }
  }
  return result;
}
```

- [ ] **Step 3: 创建 pluginInvoke.ts**

Create `src/lib/pluginInvoke.ts`:

```ts
import { invoke } from "@/lib/api";

/** 统一插件调用：pluginInvoke("filesweep", "scan:stats", {...}) */
export function pluginInvoke<T = any>(
  plugin: string,
  action: string,
  args?: Record<string, any>,
): Promise<T> {
  return invoke<T>("plugin_invoke", { plugin, action, args });
}
```

- [ ] **Step 4: 验证构建**

```bash
npm run build
```
预期：vue-tsc 通过，vite build 成功（此时无新代码引用 plugin.ts，仅定义）。

- [ ] **Step 5: 提交**

```bash
git add tsconfig.json vite.config.ts src/lib/plugin.ts src/lib/pluginInvoke.ts
git commit -m "feat(frontend): 添加 @plugins alias 与 plugin 契约定义"
```

---

### Task 9: 迁移 views/stores 到 plugins/filesweep

**Files:**
- Move: `src/views/*` → `src/plugins/filesweep/views/*`
- Move: `src/stores/*` → `src/plugins/filesweep/stores/*`

- [ ] **Step 1: 创建目标目录并移动文件**

```bash
mkdir src\plugins\filesweep\views
mkdir src\plugins\filesweep\stores
move src\views\*.vue src\plugins\filesweep\views\
move src\stores\*.ts src\plugins\filesweep\stores\
```

- [ ] **Step 2: 全局替换 import 路径**

在 `src/plugins/filesweep/` 内的文件中，`@/views/` → `@plugins/filesweep/views/`，`@/stores/` → `@plugins/filesweep/stores/`。

用编辑器全局替换（注意只替换 filesweep 目录内的引用）：
- `from "@/views/` → `from "@plugins/filesweep/views/`
- `from "@/stores/` → `from "@plugins/filesweep/stores/`

`@/lib/`、`@/components/`、`@/composables/` 保持不变（这些是宿主共享）。

- [ ] **Step 3: 验证构建**

```bash
npm run build
```
预期：若有 import 路径遗漏会报 TS 错误，按报错逐个修正。

- [ ] **Step 4: 提交**

```bash
git add -A
git commit -m "refactor(frontend): 迁移 views/stores 到 plugins/filesweep 子目录"
```

---

### Task 10: filesweep 插件 manifest + routes + nav

**Files:**
- Create: `src/plugins/filesweep/routes.ts`
- Create: `src/plugins/filesweep/nav.ts`
- Create: `src/plugins/filesweep/index.ts`
- Create: `src/plugins/_registry.ts`
- Delete: `src/router/index.ts`（内容迁入 routes.ts，router/index.ts 在 Task 11 重写）

- [ ] **Step 1: 创建 routes.ts（从原 router/index.ts 迁移）**

Create `src/plugins/filesweep/routes.ts`，把原 `src/router/index.ts` 的 routes 数组（去掉根 redirect）迁入，import 路径改为新位置：

```ts
import type { RouteRecordRaw } from "vue-router";

export const routes: RouteRecordRaw[] = [
  {
    path: "/files",
    name: "Files",
    component: () => import("@plugins/filesweep/views/FileListView.vue"),
    meta: { title: "文件管理" },
  },
  {
    path: "/scan",
    name: "Scan",
    component: () => import("@plugins/filesweep/views/ScanView.vue"),
    meta: { title: "扫描文件" },
  },
  {
    path: "/catalog",
    name: "Catalog",
    component: () => import("@plugins/filesweep/views/CatalogView.vue"),
    meta: { title: "文件目录" },
  },
  {
    path: "/enrich",
    name: "Enrich",
    component: () => import("@plugins/filesweep/views/EnrichView.vue"),
    meta: { title: "文件丰富" },
  },
  {
    path: "/tags",
    name: "Tags",
    component: () => import("@plugins/filesweep/views/TagsView.vue"),
    meta: { title: "标签管理" },
  },
  {
    path: "/categories",
    name: "Categories",
    component: () => import("@plugins/filesweep/views/CategoriesView.vue"),
    meta: { title: "分类管理" },
  },
  {
    path: "/logs",
    name: "Logs",
    component: () => import("@plugins/filesweep/views/LogsView.vue"),
    meta: { title: "操作日志" },
  },
  {
    path: "/settings",
    name: "Settings",
    component: () => import("@plugins/filesweep/views/SettingsView.vue"),
    meta: { title: "设置" },
  },
];
```

- [ ] **Step 2: 创建 nav.ts（从原 App.vue 迁移导航）**

Create `src/plugins/filesweep/nav.ts`。从 `src/App.vue` 的 `mainNavItems` / `bottomNavItems` 提取，转为 NavGroup[]。注意：原 App.vue 的「分类」组是动态从 `settingsStore.rules` 渲染的，nav.ts 只提供静态部分，动态分类组由 Sidebar 组件处理（或作为 filesweep 的动态 navGroups 函数）。

为简化 P1，nav.ts 提供静态两组（文件 + 工具），分类组保留在 AppShell 动态渲染：

```ts
import type { NavGroup } from "@/lib/plugin";

export const navGroups: NavGroup[] = [
  {
    title: "文件",
    items: [
      { label: "全部文件", icon: "Folder", route: "/files", query: {} },
      { label: "重复文件", icon: "Copy", route: "/files", query: { dup: "1" } },
      { label: "多版本", icon: "Layers", route: "/files", query: { mv: "1" } },
    ],
  },
  {
    title: "工具",
    items: [
      { label: "扫描", icon: "Scan", route: "/scan" },
      { label: "软件目录", icon: "BookOpen", route: "/catalog" },
      { label: "AI丰富", icon: "Sparkles", route: "/enrich" },
      { label: "分类管理", icon: "FolderOpen", route: "/categories" },
      { label: "标签管理", icon: "Tag", route: "/tags" },
      { label: "操作日志", icon: "ScrollText", route: "/logs" },
      { label: "设置", icon: "Settings", route: "/settings" },
    ],
  },
];
```

> 注：badge（重复文件计数）需动态，P1 先省略或改为 AppShell 内特殊处理。记录为 P2 优化项。

- [ ] **Step 3: 创建 index.ts（manifest）**

Create `src/plugins/filesweep/index.ts`:

```ts
import { definePlugin } from "@/lib/plugin";
import { routes } from "./routes";
import { navGroups } from "./nav";

export default definePlugin({
  id: "filesweep",
  name: "文件整理",
  icon: "Folder",
  pluginType: "ui",
  features: [
    { code: "files", explain: "全部文件", route: "/files", cmds: ["文件", "files"] },
    { code: "scan", explain: "扫描文件", route: "/scan", cmds: ["扫描", "scan"] },
    { code: "dedup", explain: "重复文件", route: "/files", cmds: ["去重", "重复", "dedup"] },
    { code: "catalog", explain: "软件目录", route: "/catalog", cmds: ["目录", "catalog"] },
    { code: "enrich", explain: "AI 丰富", route: "/enrich", cmds: ["AI", "丰富", "enrich"] },
  ],
  navGroups,
  routes: () => Promise.resolve(routes),
  permissions: ["*"],
});
```

- [ ] **Step 4: 创建 _registry.ts**

Create `src/plugins/_registry.ts`:

```ts
// 副作用导入：触发各插件的 definePlugin 注册
import "./filesweep";
// 未来：import "./dev_env";
```

- [ ] **Step 5: 验证构建**

```bash
npm run build
```
预期：成功（此时旧 router/index.ts 还在，可能双重定义路由。Task 11 重写 router 后正常）。

- [ ] **Step 6: 提交**

```bash
git add src/plugins/
git commit -m "feat(plugins): filesweep 插件 manifest + routes + nav + 注册表"
```

---

### Task 11: 重写 router 为动态组装

**Files:**
- Modify: `src/router/index.ts`

- [ ] **Step 1: 重写 router/index.ts**

Replace `src/router/index.ts` 全文：

```ts
import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import "@/plugins/_registry"; // 副作用：注册所有插件
import { getPlugins } from "@/lib/plugin";

async function buildRoutes(): Promise<RouteRecordRaw[]> {
  const allRoutes: RouteRecordRaw[] = [
    { path: "/", redirect: "/files" },
  ];
  for (const plugin of getPlugins()) {
    if (plugin.pluginType === "ui" && plugin.routes) {
      const pluginRoutes = await plugin.routes();
      allRoutes.push(...pluginRoutes);
    }
  }
  return allRoutes;
}

const router = createRouter({
  history: createWebHistory(),
  routes: [],
});

// 异步填充路由（buildRoutes 完成前若发生导航会等待）
buildRoutes().then((routes) => {
  for (const r of routes) {
    router.addRoute(r);
  }
});

router.beforeEach((to, _from, next) => {
  const title = to.meta.title as string | undefined;
  if (title) {
    document.title = `${title} - FileSweep`;
  }
  next();
});

export default router;
```

- [ ] **Step 2: 验证构建**

```bash
npm run build
```
预期：成功。

- [ ] **Step 3: 提交**

```bash
git add src/router/index.ts
git commit -m "refactor(router): 改为从插件注册表动态组装路由"
```

---

### Task 12: AppShell + Sidebar + iconMap

**Files:**
- Create: `src/shell/iconMap.ts`
- Create: `src/shell/Sidebar.vue`
- Create: `src/shell/AppShell.vue`
- Modify: `src/App.vue`

- [ ] **Step 1: 创建 iconMap.ts**

Create `src/shell/iconMap.ts`（lucide 图标名 → 组件映射，供 Sidebar/CommandPalette 动态用）：

```ts
import {
  Folder, FolderOpen, Search, Scan, Tag, BookOpen, Sparkles,
  Settings, ScrollText, ChevronLeft, ChevronRight, Copy, Layers,
  Minus, Square, X, Menu,
} from "lucide-vue-next";
import type { Component } from "vue";

export const iconMap: Record<string, Component> = {
  Folder, FolderOpen, Search, Scan, Tag, BookOpen, Sparkles,
  Settings, ScrollText, ChevronLeft, ChevronRight, Copy, Layers,
  Minus, Square, X, Menu,
};
```

- [ ] **Step 2: 创建 Sidebar.vue**

Create `src/shell/Sidebar.vue`。从原 App.vue 提取侧栏逻辑，改为遍历插件 navGroups：

```vue
<script setup lang="ts">
import { computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import { getPlugins } from "@/lib/plugin";
import { iconMap } from "./iconMap";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";

const router = useRouter();
const route = useRoute();

const plugins = getPlugins();

function isActive(path: string, query?: Record<string, string>) {
  if (route.path !== path) return false;
  if (!query) return true;
  return Object.entries(query).every(([k, v]) => route.query[k] === v);
}

function navigateTo(path: string, query?: Record<string, string>) {
  if (query && Object.keys(query).length > 0) {
    router.push({ path, query });
  } else {
    router.push(path);
  }
}

function getIcon(name: string) {
  return iconMap[name] ?? iconMap.Folder;
}
</script>

<template>
  <aside
    :class="[
      'flex flex-col border-r bg-card transition-all duration-200',
      $attrs.class,
    ]"
  >
    <ScrollArea class="flex-1">
      <template v-for="plugin in plugins" :key="plugin.id">
        <div v-for="group in plugin.navGroups" :key="group.title" class="p-3">
          <p class="text-xs text-muted-foreground mb-2 px-1">{{ group.title }}</p>
          <div class="space-y-0.5">
            <button
              v-for="item in group.items"
              :key="item.label"
              :class="[
                'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                isActive(item.route, item.query)
                  ? 'bg-primary text-primary-foreground'
                  : 'hover:bg-accent text-foreground',
              ]"
              @click="navigateTo(item.route, item.query)"
            >
              <component :is="getIcon(item.icon)" class="h-4 w-4" />
              <span>{{ item.label }}</span>
            </button>
          </div>
          <Separator class="my-1" />
        </div>
      </template>
    </ScrollArea>
  </aside>
</template>
```

- [ ] **Step 3: 创建 AppShell.vue**

Create `src/shell/AppShell.vue`。从原 App.vue 提取标题栏、主体布局、右侧面板、窗口控制，集成 Sidebar，预留 CommandPalette 挂载点：

```vue
<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "@plugins/filesweep/stores/settings";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { isHeadless } from "@/headless-patch";
import Sidebar from "./Sidebar.vue";
import CommandPalette from "./CommandPalette.vue";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { TooltipProvider } from "radix-vue";
import { ChevronLeft, ChevronRight, Folder, Minus, Square, X } from "lucide-vue-next";

const settingsStore = useSettingsStore();
const filesStore = useFilesStore();
const headless = isHeadless();

let appWindow: any = null;
try {
  appWindow = getCurrentWindow();
} catch {
  // headless 模式下无窗口
}

const paletteOpen = ref(false);
const rightPanelOpen = ref(true);
const sidebarCollapsed = ref(false);

const ruleItems: { key: keyof typeof settingsStore.config.rules; label: string }[] = [
  { key: "auto_categorize", label: "安装包归类" },
  { key: "auto_duplicate", label: "自动去重" },
  { key: "keep_newest_version", label: "版本保留最新" },
  { key: "move_to_recycle_bin", label: "移至回收站" },
  { key: "delete_empty_dirs", label: "删除空目录" },
];

async function minimizeWindow() { await appWindow?.minimize(); }
async function maximizeWindow() { await appWindow?.toggleMaximize(); }
async function closeWindow() { await appWindow?.close(); }

const unlisteners = ref<UnlistenFn[]>([]);

onMounted(async () => {
  await settingsStore.fetchSettings();
  await settingsStore.fetchRules();
  await filesStore.setupListeners();
  const un1 = await listen("scan_complete", () => filesStore.fetchStats());
  const un2 = await listen("clean_complete", () => {
    filesStore.fetchStats();
    filesStore.fetchFiles();
  });
  const un3 = await listen("enrich_complete", () => {});
  unlisteners.value = [un1, un2, un3];
});

onUnmounted(() => {
  filesStore.cleanupListeners();
  unlisteners.value.forEach((fn) => fn());
});
</script>

<template>
  <TooltipProvider>
    <div class="flex flex-col h-screen bg-background">
      <!-- Custom Title Bar -->
      <div
        v-if="!headless"
        class="flex items-center h-8 bg-card border-b shrink-0 select-none"
        data-tauri-drag-region
      >
        <div class="flex items-center gap-2 px-3" data-tauri-drag-region>
          <Folder class="h-4 w-4 text-primary" />
          <span class="text-xs font-semibold">FileSweep</span>
        </div>
        <div class="flex-1" data-tauri-drag-region />
        <div class="flex items-center h-full">
          <button class="flex items-center justify-center w-11 h-full hover:bg-accent transition-colors" @click="minimizeWindow">
            <Minus class="h-3.5 w-3.5" />
          </button>
          <button class="flex items-center justify-center w-11 h-full hover:bg-accent transition-colors" @click="maximizeWindow">
            <Square class="h-3 w-3" />
          </button>
          <button class="flex items-center justify-center w-11 h-full hover:bg-red-500 hover:text-white transition-colors" @click="closeWindow">
            <X class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      <!-- Main Content -->
      <div class="flex flex-1 overflow-hidden">
        <Sidebar :class="sidebarCollapsed ? 'w-0 overflow-hidden' : 'w-[200px]'" />

        <!-- Sidebar Toggle -->
        <button
          class="flex items-center justify-center w-5 border-r bg-card hover:bg-accent transition-colors"
          @click="sidebarCollapsed = !sidebarCollapsed"
        >
          <ChevronLeft v-if="!sidebarCollapsed" class="h-3 w-3" />
          <ChevronRight v-else class="h-3 w-3" />
        </button>

        <!-- Center Content -->
        <main class="flex-1 flex flex-col overflow-hidden">
          <div class="flex-1 overflow-auto">
            <router-view />
          </div>
        </main>

        <!-- Right Panel -->
        <aside v-if="rightPanelOpen" class="w-[210px] border-l bg-card flex flex-col">
          <div class="flex items-center justify-between px-4 h-12 border-b">
            <span class="text-sm font-medium">自动化规则</span>
            <button class="text-muted-foreground hover:text-foreground" @click="rightPanelOpen = false">
              <ChevronRight class="h-4 w-4" />
            </button>
          </div>
          <ScrollArea class="flex-1 p-3">
            <div class="space-y-3">
              <div v-for="item in ruleItems" :key="item.key" class="flex items-center justify-between gap-2">
                <span class="text-sm text-foreground">{{ item.label }}</span>
                <Switch
                  :model-value="settingsStore.config.rules[item.key] as boolean"
                  @update:model-value="() => settingsStore.toggleRule(item.key)"
                />
              </div>
            </div>
          </ScrollArea>
        </aside>

        <button
          v-if="!rightPanelOpen"
          class="flex items-center justify-center w-5 border-l bg-card hover:bg-accent transition-colors"
          @click="rightPanelOpen = true"
        >
          <ChevronLeft class="h-3 w-3" />
        </button>
      </div>

      <!-- Command Palette -->
      <CommandPalette v-model:open="paletteOpen" />
    </div>
  </TooltipProvider>
</template>
```

- [ ] **Step 4: 简化 App.vue 为挂载 AppShell**

Replace `src/App.vue` 全文：

```vue
<script setup lang="ts">
import AppShell from "@/shell/AppShell.vue";
</script>

<template>
  <AppShell />
</template>
```

- [ ] **Step 5: 验证构建**

```bash
npm run build
```
预期：vue-tsc 通过。若报 `CommandPalette` 找不到（Task 13 才创建），先创建一个空的 CommandPalette 占位：

```vue
<!-- src/shell/CommandPalette.vue 临时占位，Task 13 实现 -->
<script setup lang="ts">
defineProps<{ open: boolean }>();
defineEmits<{ "update:open": [boolean] }>();
</script>
<template></template>
```

- [ ] **Step 6: 提交**

```bash
git add src/shell/ src/App.vue
git commit -m "feat(shell): AppShell + 动态 Sidebar（取代硬编码导航）"
```

---

### Task 13: CommandPalette + 全局快捷键

**Files:**
- Modify: `src/shell/CommandPalette.vue`（替换 Task 12 的占位）
- Modify: `src/shell/AppShell.vue`（注册全局快捷键）

- [ ] **Step 1: 实现 CommandPalette.vue**

Replace `src/shell/CommandPalette.vue` 全文：

```vue
<script setup lang="ts">
import { ref, computed, watch, nextTick } from "vue";
import { useRouter } from "vue-router";
import { getAllFeatures, type SearchableFeature } from "@/lib/plugin";
import { iconMap } from "./iconMap";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ "update:open": [boolean] }>();

const query = ref("");
const selectedIndex = ref(0);
const inputEl = ref<HTMLInputElement | null>(null);
const router = useRouter();

const results = computed<SearchableFeature[]>(() => {
  const q = query.value.trim().toLowerCase();
  const all = getAllFeatures();
  if (!q) return all;
  return all.filter(
    (f) =>
      f.cmds.some((c) => c.toLowerCase().includes(q)) ||
      f.explain.toLowerCase().includes(q) ||
      f.pluginName.toLowerCase().includes(q),
  );
});

watch(
  () => props.open,
  async (open) => {
    if (open) {
      query.value = "";
      selectedIndex.value = 0;
      await nextTick();
      inputEl.value?.focus();
    }
  },
);

watch(results, () => {
  selectedIndex.value = 0;
});

function activate(feature: SearchableFeature) {
  if (feature.route) {
    router.push(feature.route);
  }
  emit("update:open", false);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex.value =
      (selectedIndex.value - 1 + results.value.length) % results.value.length;
  } else if (e.key === "Enter") {
    e.preventDefault();
    const target = results.value[selectedIndex.value];
    if (target) activate(target);
  } else if (e.key === "Escape") {
    emit("update:open", false);
  }
}

function getIcon(name: string) {
  return iconMap[name] ?? iconMap.Folder;
}
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-start justify-center pt-[15vh] bg-black/40"
    @click.self="emit('update:open', false)"
  >
    <div class="w-[480px] max-w-[90vw] bg-card rounded-lg shadow-xl border overflow-hidden">
      <input
        ref="inputEl"
        v-model="query"
        @keydown="onKeydown"
        placeholder="输入关键词搜索功能…"
        class="w-full px-4 py-3 bg-transparent border-b outline-none text-sm"
      />
      <div class="max-h-[320px] overflow-auto">
        <div
          v-for="(f, i) in results"
          :key="f.pluginId + f.code"
          :class="[
            'flex items-center gap-3 px-4 py-2.5 cursor-pointer text-sm',
            i === selectedIndex ? 'bg-accent' : 'hover:bg-accent/50',
          ]"
          @click="activate(f)"
          @mouseenter="selectedIndex = i"
        >
          <component :is="getIcon(f.pluginIcon)" class="h-4 w-4 text-primary shrink-0" />
          <div class="flex-1 min-w-0">
            <div class="truncate">{{ f.explain }}</div>
            <div class="text-xs text-muted-foreground truncate">{{ f.pluginName }}</div>
          </div>
          <div class="flex gap-1 shrink-0">
            <span
              v-for="c in f.cmds.slice(0, 2)"
              :key="c"
              class="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground"
            >{{ c }}</span>
          </div>
        </div>
        <div v-if="results.length === 0" class="px-4 py-8 text-center text-sm text-muted-foreground">
          无匹配功能
        </div>
      </div>
    </div>
  </div>
</template>
```

- [ ] **Step 2: AppShell 注册全局快捷键**

Modify `src/shell/AppShell.vue` 的 `<script setup>`，在 onMounted 内添加全局快捷键注册：

在 import 区添加：
```ts
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
```

在 onMounted 内末尾添加：
```ts
// 注册全局快捷键 Alt+Space 唤起命令面板
try {
  await register("Alt+Space", () => {
    paletteOpen.value = !paletteOpen.value;
  });
} catch (e) {
  console.warn("global shortcut register failed:", e);
}
```

在 onUnmounted 内添加：
```ts
try { await unregister("Alt+Space"); } catch {}
```

- [ ] **Step 3: 验证构建**

```bash
npm run build
```
预期：vue-tsc 通过，vite build 成功。

- [ ] **Step 4: 提交**

```bash
git add src/shell/CommandPalette.vue src/shell/AppShell.vue
git commit -m "feat(shell): 命令面板（Alt+Space 全局唤起，按 feature 搜索）"
```

---

### 📋 阶段 B 检查点

- [ ] **Step 1: npm run build 通过**

- [ ] **Step 2: 手动走查（用户在有 Rust + Tauri 环境的机器）**

运行 `npm run tauri dev`，验证：
1. 应用启动，侧栏从插件 manifest 动态渲染（文件/工具两组）
2. 全部文件 / 扫描 / 目录 / AI / 标签 / 分类 / 日志 / 设置 全部可访问
3. 扫描功能正常（触发 scan:start，等价原 start_scan）
4. 右侧自动化规则面板可切换
5. 按 `Alt+Space` 唤起命令面板
6. 输入「扫描」命中 scan feature，回车进入扫描页
7. 输入「去重」命中 dedup feature，回车进入文件页
8. Esc 关闭面板
9. DevTools 调用 `await window.__TAURI__.core.invoke("plugin_list")` 返回 filesweep

---

## 验收标准（对齐设计文档第 7 节）

- [ ] 后端 `cargo check` 通过（用户在 Rust 环境验证）
- [ ] `plugin_list` 返回 filesweep metadata（5 features）
- [ ] `plugin_invoke("filesweep","scan:stats",{})` 等价 `get_file_stats`
- [ ] `npm run build` 通过
- [ ] filesweep 全功能正常（扫描/目录/AI/标签/分类/设置/日志）
- [ ] 侧栏从插件 manifest 动态渲染
- [ ] Alt+Space 命令面板可用，关键词直达路由
- [ ] 现有 26 命令仍可通过旧 invoke() 调用
- [ ] src/views、src/stores 迁入 src/plugins/filesweep/

---

## 已知遗留（P2 处理）

1. **filesweep actions 未全覆盖**：Task 6 只适配 scan/catalog，其余 action（clean/enrich/settings/rules/categories/tags/logs/db）返回 NotImplemented。双轨并存下不影响主功能（stores 仍用旧 invoke），P2 补齐剩余 _impl 提取与 action 适配。
2. **stores 未迁移到 pluginInvoke**：P1 保留 stores 内 `invoke("scan:start")` 旧调用。P2/P3 渐进切换为 `pluginInvoke("filesweep","scan:start")`。
3. **nav badge（重复文件计数）**：P1 省略动态角标。P2 让 NavItem.badge 生效。
4. **动态分类导航组**：原 App.vue 的「分类」组从 settingsStore.rules 动态渲染，P1 nav.ts 只含静态组。P2 加动态 navGroups 支持。

---

## 回滚指引

阶段 A（后端）若有问题：
```bash
git revert <task7-commit>..<HEAD>   # 回滚后端整合
# 现有 26 命令不受影响，应用仍可用旧架构
```

阶段 B（前端）若有问题：
```bash
git revert <task9-commit>..<HEAD>   # 回滚前端迁移
git checkout HEAD~<n> -- src/App.vue src/router/index.ts  # 恢复原 App.vue 和 router
```

双轨并存设计确保任何阶段回滚都不破坏 filesweep 核心功能。
