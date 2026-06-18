# P1 插件骨架设计

**日期**：2026-06-18
**状态**：已批准
**关联文档**：`docs/superpowers/specs/2026-06-18-plugin-platform-migration-design.md`（总设计）
**参考项目**：
- [rubick](https://github.com/rubickCenter/rubick)（半兼容 uTools 的 Electron 插件工具箱）——借鉴 features[] 多入口、pluginType 二分
- [uTools 开发者文档](https://www.u-tools.cn/docs/developer/)——关键词触发模型
- [kunkun](https://github.com/kunkunsh/kunkun)（Tauri+SvelteKit 启动器）——同技术栈参考，借鉴权限模型预留、FeatureType 多类型预留

---

## 1. 目标与范围

### 1.1 目标
建立可扩展的插件化内核，使 FileSweep 从单功能工具升级为类 uTools 的本地系统优化平台宿主。现有 filesweep 功能作为首个内置插件接入，端到端验证骨架可用性。

### 1.2 P1 交付
- 后端：`Plugin` trait + `PluginHost` 注册表 + `plugin_invoke` / `plugin_list` 分发命令
- 后端：`FileSweepPlugin` 适配层（转发现有 26 个命令）
- 前端：`definePlugin` 契约 + features 多入口 + 动态路由组装
- 前端：filesweep 现有 views/stores/router 迁入 `src/plugins/filesweep/`
- 前端：`AppShell` + 动态 `Sidebar`（取代硬编码导航）
- 前端：`CommandPalette`（Alt+Space 全局唤起，按 feature 搜索）

### 1.3 不在 P1 范围
- 现有 stores 内 `invoke()` 调用的全量迁移到 `pluginInvoke`（向后兼容保留，渐进迁移留 P2/P3）
- headless 模式的插件化适配（留 P3）
- 动态加载外部插件包（留 P5）
- 命令面板的历史/收藏/fuzzy 匹配（P1 用简单 includes）

---

## 2. 关键决策（已确认）

| 决策点 | 选择 | 理由 |
|---|---|---|
| 命令注册机制 | 统一分发器 `plugin_invoke` | Tauri `generate_handler!` 是编译期宏，分发器保留现有命令同时支持新插件 |
| P1 范围 | 骨架 + filesweep 适配 | 端到端可见，降低骨架设计偏差风险 |
| 命令面板 + 侧栏 | 两者都做 | 完整 uTools 体验 |
| 前端插件边界 | 独立目录 `src/plugins/filesweep/` | 边界清晰，新插件可复制结构 |
| 插件入口模型 | **features[] 多入口**（借鉴 rubick/uTools） | 一个插件可被多个关键词触发直达不同子路由 |
| 插件类型 | **预留 `ui` / `system`**（借鉴 rubick） | system 插件支持未来后台任务，trait 设计预留 |
| 命令面板搜索粒度 | **按 feature 搜索** | 输入「去重」直达去重页，最贴 uTools 体验 |
| 向后兼容 | 现有 26 命令 + invoke 调用保留 | 分发器与旧命令并存，stores 渐进迁移 |
| headless 适配 | P1 不做 | headless 用户少，留 P3 |

---

## 3. 借鉴 rubick/uTools 的设计要点

研究 rubick 后提取的核心模型，已融入本设计：

| rubick/uTools 机制 | 我们的处理 |
|---|---|
| `pluginType: "ui" \| "system"` | ✅ 采用——trait 预留 `PluginType` 枚举 |
| `features[]` 多入口（code/explain/cmds） | ✅ 采用——`PluginFeature { code, explain, route, cmds }` |
| `preload.js` 注入全局 API | ❌ 不采用——改用 ES module 导入 `pluginInvoke`，更安全、符合 Vue/TS 生态 |
| npm 包管理 + 运行时加载 | ❌ 不采用（P1）——内置插件随主程序编译，动态加载留 P5 |
| logo 必须在线 URL | ❌ 不采用——本地 lucide 图标名，离线优先 |
| 开发者模式本地插件路径调试 | 📌 记录——P5 动态加载时参考 |

### 3.1 kunkun 启发的预留字段（Tauri 同技术栈参考）

研究 kunkun 后，额外采纳两个**增量预留**（不改变 P1 核心行为，为 P5 扩展铺路）：

| kunkun 机制 | 我们的处理 |
|---|---|
| 多级权限模型 + Tauri capabilities + `tauri-plugin-shellx-api` | ✅ **预留 permissions 字段**——P1 内置插件默认全权限（可信），P5 第三方插件显式声明能力 |
| Template UI / Custom UI / Headless 三类命令 | ✅ **预留 FeatureType 枚举**——P1 全是 route 类型，未来支持 template（宿主渲染表单）和 action（纯命令无路由） |
| 扩展即 npm 包 + 独立 webview 沙箱 | ❌ P1 不采用——内置 Rust 模块，动态加载与窗口隔离留 P5 |
| `kksh verify --publish` 校验 + provenance | 📌 记录——P5 扩展商店发布流程参考 |

**三项目横向对比：**

| 维度 | rubick (Electron) | kunkun (Tauri) | 我们 P1 (Tauri) |
|---|---|---|---|
| 扩展形态 | npm 包运行时加载 | npm 包 + webview 沙箱 | 内置 Rust 模块（P5 动态加载） |
| UI 隔离 | 共享渲染进程 | 独立 webview | 主窗口内路由 |
| 权限模型 | 无显式 | 多级 + capabilities | 预留字段（P1 默认 All） |
| 命令类型 | ui/system 二分 | Template/Custom/Headless 三类 | route/template/action 预留 |

---

## 4. 后端设计

### 4.1 模块结构

```
src-tauri/src/
├── app/                        # 新增：宿主内核
│   ├── mod.rs
│   ├── plugin.rs               # Plugin trait + PluginMetadata + PluginFeature + PluginType
│   ├── context.rs              # Context（共享状态：db/config/app_handle）
│   ├── host.rs                 # PluginHost 注册表 + dispatch
│   └── ipc.rs                  # plugin_invoke / plugin_list Tauri 命令
├── plugins/                    # 新增：内置插件
│   ├── mod.rs                  # register_all() 汇总
│   └── filesweep/
│       ├── mod.rs              # FileSweepPlugin 实现 Plugin trait
│       └── actions.rs          # action → 现有 commands 函数的转发映射
├── commands/                   # 保留不动
├── core/                       # 保留不动
├── db/                         # 保留不动
├── ai/                         # 保留不动
├── headless.rs                 # 保留不动（P1 不适配插件）
├── lib.rs                      # 修改：注册 PluginHost + 2 个新命令
└── main.rs                     # 保留不动
```

### 4.2 Plugin trait（`app/plugin.rs`）

```rust
use serde::{Serialize, Deserialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum PluginType {
    Ui,
    System,
}

/// feature 类型（kunkun 启发的预留）
/// - Route: 进入路由（P1 全部此类型）
/// - Template: 宿主渲染表单（P5 扩展）
/// - Action: 纯命令无路由，如「一键清理」（P5 扩展）
#[derive(Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum FeatureType {
    Route,
    Template,
    Action,
}

impl Default for FeatureType {
    fn default() -> Self { FeatureType::Route }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginFeature {
    pub code: String,       // "scan" —— feature 标识，用于路由激活
    pub explain: String,    // "扫描文件" —— 命令面板显示文案
    pub cmds: Vec<String>,  // ["扫描", "scan"] —— 触发关键词
    #[serde(default)]
    pub feature_type: FeatureType,   // 默认 Route
    pub route: Option<String>,       // Route 类型必填，如 "/scan"
}

/// 插件权限声明（kunkun 启发的预留）
/// P1 内置插件默认 All，P5 第三方插件显式声明能力
#[derive(Clone, Serialize, Deserialize, Default)]
pub struct PluginPermissions {
    /// 声明能力，如 ["fs:read", "fs:write", "shell:exec", "net:fetch"]
    /// 空或 ["*"] 表示全部权限（P1 内置插件）
    #[serde(default)]
    pub capabilities: Vec<String>,
}

impl PluginPermissions {
    pub fn all() -> Self {
        Self { capabilities: vec!["*".into()] }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    pub id: String,                 // "filesweep"
    pub name: String,               // "文件整理"
    pub icon: String,               // lucide 图标名，如 "Folder"
    pub plugin_type: PluginType,
    pub features: Vec<PluginFeature>,   // ui 插件有，system 插件为空
    pub version: String,
    #[serde(default = "PluginPermissions::all")]
    pub permissions: PluginPermissions, // 默认 All（P1 内置可信）
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
    fn from(e: serde_json::Error) -> Self { PluginError::Internal(e.to_string()) }
}

#[async_trait::async_trait]
pub trait Plugin: Send + Sync {
    fn metadata(&self) -> &PluginMetadata;

    /// 声明支持的 action（用于校验/文档）
    fn actions(&self) -> Vec<&'static str> { vec![] }

    /// 处理 invoke：action 为命令名（如 "scan:start"），args 为参数
    async fn invoke(&self, action: &str, args: Value, ctx: &Context)
        -> Result<Value, PluginError>;

    /// system 插件的启动钩子（ui 插件默认空实现）
    async fn on_start(&self, _ctx: &Context) -> Result<(), PluginError> { Ok(()) }
}
```

### 4.3 Context（`app/context.rs`）

复用现有共享状态，插件通过它访问 db/config，无需改造内部。

```rust
use std::sync::Arc;
use parking_lot::RwLock;

pub struct Context {
    pub db: Arc<crate::db::catalog::CatalogDB>,
    pub config: Arc<RwLock<crate::core::config::Config>>,
    pub app_handle: tauri::AppHandle,
}
```

### 4.4 PluginHost + 分发（`app/host.rs`）

```rust
use std::collections::HashMap;
use std::sync::Arc;

pub struct PluginHost {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginHost {
    pub fn new() -> Self { Self { plugins: HashMap::new() } }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        let id = plugin.metadata().id.clone();
        self.plugins.insert(id, plugin);
    }

    /// 返回所有插件元信息（前端 plugin_list 用）
    pub fn metadata_list(&self) -> Vec<PluginMetadata> {
        self.plugins.values().map(|p| p.metadata().clone()).collect()
    }

    /// 启动所有 system 插件
    pub async fn start_system_plugins(&self, ctx: &Context) -> Result<(), PluginError> {
        for p in self.plugins.values() {
            if p.metadata().plugin_type == PluginType::System {
                p.on_start(ctx).await?;
            }
        }
        Ok(())
    }

    /// 分发 invoke
    pub async fn dispatch(
        &self, plugin_id: &str, action: &str, args: Value, ctx: &Context,
    ) -> Result<Value, PluginError> {
        let plugin = self.plugins.get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.into()))?;
        plugin.invoke(action, args, ctx).await
    }
}
```

### 4.5 IPC 命令（`app/ipc.rs`）

```rust
use tauri::State;
use serde_json::Value;

#[tauri::command]
pub async fn plugin_invoke(
    plugin: String,
    action: String,
    args: Option<Value>,
    host: State<'_, Arc<PluginHost>>,
    db: State<'_, Arc<crate::db::catalog::CatalogDB>>,
    config: State<'_, Arc<parking_lot::RwLock<crate::core::config::Config>>>,
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

#[tauri::command]
pub fn plugin_list(host: State<'_, Arc<PluginHost>>) -> Vec<PluginMetadata> {
    host.metadata_list()
}
```

### 4.6 FileSweepPlugin 适配层（`plugins/filesweep/mod.rs`）

**关键**：现有 `commands/*.rs` 内部逻辑零改动，插件只是薄转发层。

```rust
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
                        code: "files".into(), explain: "全部文件".into(),
                        route: Some("/files".into()), cmds: vec!["文件".into(), "files".into()],
                        feature_type: FeatureType::Route,
                    },
                    PluginFeature {
                        code: "scan".into(), explain: "扫描文件".into(),
                        route: Some("/scan".into()), cmds: vec!["扫描".into(), "scan".into()],
                        feature_type: FeatureType::Route,
                    },
                    PluginFeature {
                        code: "dedup".into(), explain: "重复文件".into(),
                        route: Some("/files".into()), cmds: vec!["去重".into(), "重复".into(), "dedup".into()],
                        feature_type: FeatureType::Route,
                    },
                    PluginFeature {
                        code: "catalog".into(), explain: "软件目录".into(),
                        route: Some("/catalog".into()), cmds: vec!["目录".into(), "catalog".into()],
                        feature_type: FeatureType::Route,
                    },
                    PluginFeature {
                        code: "enrich".into(), explain: "AI 丰富".into(),
                        route: Some("/enrich".into()), cmds: vec!["AI".into(), "丰富".into(), "enrich".into()],
                        feature_type: FeatureType::Route,
                    },
                ],
                version: env!("CARGO_PKG_VERSION").into(),
                permissions: PluginPermissions::all(),
            },
        }
    }
}

#[async_trait::async_trait]
impl Plugin for FileSweepPlugin {
    fn metadata(&self) -> &PluginMetadata { &self.meta }

    fn actions(&self) -> Vec<&'static str> {
        vec![
            "scan:start", "scan:files", "scan:stats", "scan:suggestions",
            "clean:start", "catalog:get", "catalog:update", "catalog:delete",
            "catalog:export", "enrich:start", "enrich:status",
            "settings:get", "settings:update", "rules:get", "rules:update",
            "categories:get", "categories:update",
            "tags:get", "tags:create", "tags:update", "tags:delete",
            "logs:get", "logs:revert", "logs:batch_revert", "db:reset",
        ]
    }

    async fn invoke(&self, action: &str, args: Value, ctx: &Context)
        -> Result<Value, PluginError>
    {
        crate::plugins::filesweep::actions::dispatch(action, args, ctx).await
    }
}
```

### 4.7 actions 转发（`plugins/filesweep/actions.rs`）

action 字符串映射到现有 commands 函数。每个 action 一行转发，参数反序列化后调用原函数。

```rust
pub async fn dispatch(action: &str, args: Value, ctx: &Context)
    -> Result<Value, PluginError>
{
    let result = match action {
        "scan:start" => {
            let parsed: ScanArgs = serde_json::from_value(args)?;
            serde_json::to_value(
                crate::commands::scan::start_scan(parsed, /* &ctx.db, &ctx.config */).await?
            )?
        }
        // ... 其余 25 个 action 同样模式 ...
        _ => return Err(PluginError::UnknownAction(action.into())),
    };
    Ok(result)
}
```

> **注意**：现有 commands 函数签名可能需要微调以接受 `Context` 引用，但核心逻辑不变。实现阶段逐个 action 处理，遇到签名不兼容就包一层适配。

### 4.8 lib.rs 改造

```rust
pub mod app;
pub mod plugins;

pub fn run() {
    // ... 现有 config/db 初始化不变 ...

    // 新增：构建 PluginHost
    let mut host = app::host::PluginHost::new();
    host.register(Box::new(plugins::filesweep::FileSweepPlugin::new()));
    let host = Arc::new(host);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())  // 新增：命令面板快捷键
        .manage(db)
        .manage(config)
        .manage(enrich_state)
        .manage(host.clone())                                           // 新增
        .invoke_handler(tauri::generate_handler![
            // 现有 26 个命令保留不变 ...
            app::ipc::plugin_invoke,                                    // 新增
            app::ipc::plugin_list,                                      // 新增
        ])
        .setup(move |app| {
            // 启动 system 插件（P1 无 system 插件，预留）
            let host = host.clone();
            tauri::async_runtime::spawn(async move {
                // host.start_system_plugins(...).await.ok();
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running FileSweep");
}
```

### 4.9 Cargo.toml 新增依赖

```toml
async-trait = "0.1"
thiserror = "1"
tauri-plugin-global-shortcut = "2"
```

---

## 5. 前端设计

### 5.1 目录重组

```
src/
├── plugins/
│   ├── _registry.ts            # 汇总：import 所有插件（副作用注册）
│   └── filesweep/
│       ├── index.ts            # definePlugin() + manifest
│       ├── routes.ts           # 从 src/router/index.ts 迁入的 8 条路由
│       ├── nav.ts              # 从 App.vue 迁入的三组导航
│       ├── views/              # 从 src/views/ 整体迁入（8 个 .vue）
│       └── stores/             # 从 src/stores/ 整体迁入（3 个 .ts）
├── shell/                      # 新增：宿主 UI
│   ├── AppShell.vue            # 取代现 App.vue 主体
│   ├── Sidebar.vue             # 从插件 manifest.navGroups 动态渲染
│   └── CommandPalette.vue      # Alt+Space 唤起，按 feature 搜索
├── lib/
│   ├── api.ts                  # 保留（headless 抽象）
│   ├── plugin.ts               # 新增：definePlugin + PluginManifest 契约
│   └── pluginInvoke.ts         # 新增：pluginInvoke 封装
├── main.ts                     # 修改：动态组装路由
├── App.vue                     # 简化为挂载 AppShell
└── router/index.ts             # 简化为动态 router（从插件收集路由）
```

### 5.2 插件契约（`src/lib/plugin.ts`）

```ts
import type { RouteRecordRaw } from "vue-router";

export type PluginType = "ui" | "system";

/// feature 类型（kunkun 启发预留）
/// - route: 进入路由（P1 全部此类型）
/// - template: 宿主渲染表单（P5 扩展）
/// - action: 纯命令无路由（P5 扩展）
export type FeatureType = "route" | "template" | "action";

export interface PluginFeature {
  code: string;           // "scan"
  explain: string;        // "扫描文件"
  cmds: string[];         // ["扫描", "scan"]
  type?: FeatureType;     // 默认 "route"
  route?: string;         // type=route 时必填，如 "/scan"
}

export interface NavItem {
  label: string;
  icon: string;
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
  /** 权限声明（kunkun 启发预留）。默认 ["*"] 全权限（P1 内置可信）。
   *  P5 第三方插件显式声明，如 ["fs:read", "shell:exec"] */
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

/** 扁平化所有 feature，附加所属插件信息（命令面板搜索用） */
export interface SearchableFeature {
  code: string;
  explain: string;
  cmds: string[];
  type?: FeatureType;
  route?: string;        // 可选：action 类型无 route
  pluginId: string;
  pluginName: string;
  pluginIcon: string;
}

export function getAllFeatures(): SearchableFeature[] {
  const result: SearchableFeature[] = [];
  for (const plugin of getPlugins()) {
    for (const feature of plugin.features) {
      // 只纳入可搜索的类型（route/template/action 都可搜索；P1 全是 route）
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

### 5.3 pluginInvoke 封装（`src/lib/pluginInvoke.ts`）

```ts
import { invoke } from "@/lib/api";

export function pluginInvoke<T = any>(
  plugin: string,
  action: string,
  args?: Record<string, any>,
): Promise<T> {
  return invoke<T>("plugin_invoke", { plugin, action, args });
}
```

> stores 渐进迁移：P1 可保留现有 `invoke("scan:start")` 调用（向后兼容），新功能用 `pluginInvoke`。

### 5.4 filesweep 插件定义（`src/plugins/filesweep/index.ts`）

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
  permissions: ["*"],   // 内置插件全权限
});
```

### 5.5 路由动态组装（`src/router/index.ts` 改造）

```ts
import { createRouter, createWebHistory, type RouteRecordRaw } from "vue-router";
import "@/plugins/_registry";  // 副作用：注册所有插件
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
  routes: [],  // 先空，异步填充
});

buildRoutes().then((routes) => {
  for (const r of routes) router.addRoute(r);
});

router.beforeEach((to, _from, next) => {
  const title = to.meta.title as string | undefined;
  if (title) document.title = `${title} - FileSweep`;
  next();
});

export default router;
```

### 5.6 注册表汇总（`src/plugins/_registry.ts`）

```ts
// 副作用导入：触发各插件的 definePlugin
import "./filesweep";
// 未来：import "./dev_env";
//       import "./env_vars";
```

### 5.7 AppShell（`src/shell/AppShell.vue`）

取代现 `App.vue` 的布局逻辑，但导航改为从插件 manifest 读取：

```vue
<script setup lang="ts">
import Sidebar from "./Sidebar.vue";
import CommandPalette from "./CommandPalette.vue";
import { ref } from "vue";
// ... 标题栏、窗口控制等逻辑从现 App.vue 迁移 ...
const paletteOpen = ref(false);
</script>

<template>
  <!-- 标题栏（从 App.vue 迁移） -->
  <!-- 主体：Sidebar + router-view + 右侧面板（从 App.vue 迁移） -->
  <Sidebar />
  <CommandPalette v-model:open="paletteOpen" />
</template>
```

### 5.8 Sidebar（`src/shell/Sidebar.vue`）

遍历所有插件的 `navGroups` 动态渲染：

```vue
<script setup lang="ts">
import { getPlugins } from "@/lib/plugin";
const plugins = getPlugins();
// 图标动态加载：维护 lucide 图标名 → 组件的映射
</script>

<template>
  <aside>
    <template v-for="plugin in plugins" :key="plugin.id">
      <template v-for="group in plugin.navGroups" :key="group.title">
        <p>{{ group.title }}</p>
        <button v-for="item in group.items" @click="navigate(item)">
          <component :is="iconMap[item.icon]" />
          {{ item.label }}
        </button>
      </template>
    </template>
  </aside>
</template>
```

> filesweep 的三组导航（文件/分类/工具）从现 `App.vue` 提取为 `navGroups`。

### 5.9 CommandPalette（`src/shell/CommandPalette.vue`）

Alt+Space 唤起，按 feature 搜索，回车进入：

```vue
<script setup lang="ts">
import { ref, computed, watch } from "vue";
import { useRouter } from "vue-router";
import { getAllFeatures, type SearchableFeature } from "@/lib/plugin";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ "update:open": [boolean] }>();

const query = ref("");
const selectedIndex = ref(0);
const router = useRouter();

const results = computed<SearchableFeature[]>(() => {
  const q = query.value.trim().toLowerCase();
  const all = getAllFeatures();
  if (!q) return all;
  return all.filter(f =>
    f.cmds.some(c => c.toLowerCase().includes(q)) ||
    f.explain.toLowerCase().includes(q) ||
    f.pluginName.toLowerCase().includes(q)
  );
});

function activate(feature: SearchableFeature) {
  // route 类型：跳转路由；action 类型未来：直接执行命令（P1 全是 route）
  if (feature.route) {
    router.push(feature.route);
  }
  // 可选：调用插件的 onActivate(feature.code)
  emit("update:open", false);
  query.value = "";
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
  if (e.key === "ArrowUp") selectedIndex.value = (selectedIndex.value - 1 + results.value.length) % results.value.length;
  if (e.key === "Enter" && results.value[selectedIndex.value]) activate(results.value[selectedIndex.value]);
  if (e.key === "Escape") emit("update:open", false);
}
</script>

<template>
  <div v-if="open" class="palette-overlay" @click.self="emit('update:open', false)">
    <div class="palette-panel">
      <input v-model="query" @keydown="onKeydown" placeholder="输入关键词搜索功能..." autofocus />
      <div class="palette-results">
        <div
          v-for="(f, i) in results" :key="f.pluginId + f.code"
          :class="['palette-item', { active: i === selectedIndex }]"
          @click="activate(f)"
        >
          <component :is="iconMap[f.pluginIcon]" />
          <div>
            <div>{{ f.explain }}</div>
            <div class="muted">{{ f.pluginName }}</div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>
```

### 5.10 全局快捷键（main.ts 或 AppShell）

```ts
import { register } from "@tauri-apps/plugin-global-shortcut";

register("Alt+Space", () => {
  paletteOpen.value = !paletteOpen.value;
});
```

### 5.11 tsconfig alias 新增

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

---

## 6. 数据流

### 6.1 插件调用流（前端 → 后端）

```
用户在 CatalogView 点击「导出」
  → store: pluginInvoke("filesweep", "catalog:export", {...})
  → invoke("plugin_invoke", {plugin:"filesweep", action:"catalog:export", args})
  → Tauri: app::ipc::plugin_invoke
  → PluginHost::dispatch("filesweep", "catalog:export", args, ctx)
  → FileSweepPlugin::invoke("catalog:export", args, ctx)
  → actions::dispatch → commands::catalog::export_catalog(...)
  → 返回 Value 链路回前端
```

### 6.2 命令面板激活流

```
用户按 Alt+Space
  → CommandPalette 显示，输入「去重」
  → getAllFeatures() 过滤 cmds 含「去重」的 feature
  → 命中 filesweep 的 dedup feature（route: "/files"）
  → 回车：router.push("/files") + 可选 plugin.onActivate("dedup")
  → 面板关闭
```

### 6.3 侧栏渲染流

```
AppShell 挂载
  → getPlugins() 返回 [filesweepPlugin]
  → Sidebar 遍历 filesweepPlugin.navGroups
  → 渲染「文件」「分类」「工具」三组导航
  → 新增 dev_env 插件时，其 navGroups 自动出现在侧栏
```

---

## 7. 验收标准

P1 完成当且仅当：

- [ ] 后端 `cargo check` 通过（含 app/ + plugins/ 新模块）
- [ ] 后端：`plugin_list` 返回含 filesweep 的 metadata（含 5 个 features）
- [ ] 后端：`plugin_invoke("filesweep", "scan:files", {})` 等价于原 `invoke("get_files")`
- [ ] 前端 `npm run build` 通过（vue-tsc 类型检查无错）
- [ ] 前端：filesweep 全部功能（扫描/目录/AI/标签/分类/设置/日志）正常工作
- [ ] 前端：侧栏从插件 manifest 动态渲染（非硬编码）
- [ ] 前端：Alt+Space 唤起命令面板，输入关键词直达对应路由
- [ ] 现有 26 个命令仍可通过旧 `invoke()` 调用（向后兼容）
- [ ] `src/views`、`src/stores` 已迁入 `src/plugins/filesweep/`

---

## 8. 风险与缓解

| 风险 | 缓解 |
|---|---|
| actions 转发层漏 action（26 个易遗漏） | 实现时用 actions() 返回值与 dispatch match 分支交叉核对；P1 验收时跑一遍全功能走查 |
| 现有 commands 函数签名不兼容 Context | 转发层做适配：从 ctx 取 db/config，按原签名构造参数调用 |
| 路由动态组装时机问题（buildRoutes 异步） | 用 router.addRoute 异步填充；或 buildRoutes 完成后再挂载 app |
| 全局快捷键冲突（Alt+Space 可能被系统占用） | tauri.conf.json 配置权限；快捷键可在设置中自定义（P1 先硬编码，P3 加设置） |
| views/stores 迁移后 import 路径全断 | 加 `@plugins` alias；用 IDE 全局替换 + 编译验证 |

---

## 9. 向后兼容说明

P1 采取**双轨并存**策略：

1. **后端**：现有 26 个命令通过 `generate_handler!` 直接注册，保持原样可用。新增 `plugin_invoke` / `plugin_list` 作为插件通道。两套命令并存。
2. **前端**：stores 可继续用 `invoke("scan:start")`（旧通道），也可用 `pluginInvoke("filesweep", "scan:start")`（新通道）。P1 不强制迁移，P2/P3 渐进切换。

这样即使分发器有问题，filesweep 核心功能仍可通过旧通道工作，降低 P1 风险。
