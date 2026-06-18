# FileSweep 插件化平台迁移设计

**日期**：2026-06-18
**状态**：已批准
**作者**：Spence + ZCode 协作
**关联文档**：`docs/archive/FileSweep_DesignDoc_v1.0.md`（原 v1.0 设计，归档）

---

## 1. 背景与现状

FileSweep 原为 Go (Cobra CLI + Gin HTTP server) + Vue3/naive-ui 的单二进制工具，用于扫描/去重/分类/AI 丰富软件文件。

现已用 **Tauri (Rust) + Vue3/radix-vue(shadcn)** 重构，旧 Go 栈与旧前端 `frontend/` 作为残留代码留在仓库中。

经讨论，项目方向发生转变：**FileSweep 将从「单功能文件整理工具」演进为「类 uTools 的本地系统优化平台」**，原文件整理功能降级为首个内置插件。后续规划落地多个插件：开发环境管理（FlyEnv 风格）、环境变量管理、用户目录备份迁移与软链接管理、重复文件清理（Czkawka 风格）。

### 双栈现状对照

| 维度 | 旧栈（待清理） | 新栈（Tauri） |
|---|---|---|
| 后端 | Go: `main.go`, `embed.go`, `cmd/`, `internal/` | Rust: `src-tauri/src/` (core/ai/commands/db/headless/cli) |
| 后端依赖 | `go.mod` / `go.sum` | `Cargo.toml` |
| 前端 | `frontend/` (Vue3 + naive-ui + axios，HTTP 调用) | `src/` (Vue3 + radix-vue/shadcn + Tauri invoke) |
| 入口 | `filesweep.exe` (CLI 二进制) + `dist/filesweep.exe` | Tauri 构建 |
| 构建 | `Makefile`, `tauri-build.bat` | `tauri-build.bat` |
| 文档 | `FileSweep_DesignDoc_v1.0.md`, `原型.html`, `绿色软件识别.md` | — |

### 功能完成度（Tauri 端）

`lib.rs` 注册的 command 与 `src/views` 对应良好：扫描、清理、目录、AI 丰富、设置、规则、分类、标签、日志回滚。P0/P1 功能在 Tauri 端已基本对应实现，重构主体完成。

---

## 2. 关键决策（已与用户确认）

1. **清理策略**：归档后删除。打 `legacy/go-stack` tag + `archive/legacy-go-stack` 分支保留可回溯，main 上删除旧 Go/旧前端/旧产物。
2. **开发重点**：功能对等补齐 + 新功能演进 + 架构/工程化，三者结合。最终目标为插件化平台。
3. **插件化形态**：**内置插件架构**（Rust trait + Vue 懒加载路由），插件随主程序编译。后续可评估动态加载。
4. **CLI 能力**：保留 Rust CLI（`src-tauri/src/cli/` + `headless.rs`），删除 Go serve 命令。

---

## 3. 旧栈清理方案（P0）

### 3.1 归档

- `git tag legacy/go-stack`（指向清理前 HEAD）
- `git branch archive/legacy-go-stack`（同上，分支形式便于检出）
- 原设计文档 `FileSweep_DesignDoc_v1.0.md` 移入 `docs/archive/`
- `原型.html`、`绿色软件识别.md` 移入 `docs/archive/`

### 3.2 删除清单

**Go 源码与构建：**
- `main.go`, `embed.go`
- `cmd/`（含 serve/scan 等子命令）
- `internal/`（server/ai/config/core/db 等 Go 实现）
- `tests/`（Go 测试）
- `go.mod`, `go.sum`
- `Makefile`

**旧前端：**
- `frontend/`（整目录；已确认新前端 `src/` 通过 `@` alias 引用自身，不依赖 `frontend/`）

**旧产物与临时文件：**
- `filesweep.exe`, `filesweep.exe~`
- `dist/filesweep.exe`
- `catalog-all.csv`
- `rustup-init.exe`
- `Users/`（误生成目录，内容为 `Users/Spence`，应为某次命令将绝对路径当作相对路径创建）

### 3.3 配置/数据库

- `config/catalog.db` 从版本控制移除并加入 `.gitignore`。理由：`src-tauri/src/core/config.rs:24` 表明数据库路径指向用户配置目录，运行时生成，仓库内的 `config/catalog.db` 是历史快照。
- `config/`（categories/rules 等静态配置）保留。

### 3.4 `.gitignore` 补充

```
node_modules
dist
src-tauri/target
*.exe
*.exe~
*.csv
config/catalog.db
docs/superpowers/specs/*.tsbuildinfo
tsconfig.tsbuildinfo
```

> 注：根 `dist/` 是 Tauri 前端构建产物（`frontendDist: "../dist"`），按惯例不入库；若需保留 release 产物再评估。

### 3.5 验证

- `cargo check`（在 `src-tauri/`）确认无引用残留
- `npm run build` 确认前端构建通过
- grep 全仓确认无 `frontend/`、`cmd/internal`、`go.mod` 残留引用

---

## 4. 插件化架构设计（核心）

### 4.1 目标

FileSweep 成为本地系统优化平台宿主，原文件整理功能成为内置插件 `filesweep`。插件采用**内置架构**（随主程序编译），通过统一契约注册，按需激活加载。

### 4.2 目录结构

```
src-tauri/src/
├── app/                        # 宿主内核
│   ├── mod.rs
│   ├── plugin_host.rs          # Plugin trait + 注册表 + 生命周期
│   └── ipc.rs                  # 统一命令分发：invoke("plugin:<id>:<action>")
├── plugins/                    # 内置插件
│   ├── mod.rs                  # register_all() 汇总
│   ├── filesweep/              # 现有功能迁入（扫描/去重/分类/目录/AI）
│   ├── dev_env/                # 开发环境管理（FlyEnv 风格）
│   ├── env_vars/               # 环境变量管理
│   ├── profile_backup/         # 用户目录备份迁移 + 软链接
│   └── dedup/                  # 重复文件清理（Czkawka 风格）
├── cli/                        # Rust CLI（保留，插件可注册子命令）
└── headless.rs

src/                            # 前端：插件式路由 + 按需加载
├── shell/                      # 宿主 UI（命令面板/侧栏/全局快捷键）
├── plugins/                    # 每插件一个懒加载路由块
│   ├── filesweep/
│   ├── dev_env/
│   └── ...
└── lib/plugin.ts               # definePlugin() 契约
```

### 4.3 插件契约（前后端统一）

**后端 trait：**

```rust
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;                      // "filesweep"
    fn metadata(&self) -> Metadata;            // 名称/图标/关键词/版本
    fn commands(&self) -> Vec<CommandSpec>;    // 暴露的 IPC actions
    fn on_activate(&self, ctx: &Context) -> Result<()>;
    fn on_deactivate(&self) -> Result<()>;
}

pub struct Metadata {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub keywords: Vec<String>,      // 命令面板关键词触发
    pub version: String,
}

pub struct CommandSpec {
    pub action: String,             // "scan:start"
    pub handler: fn(...) -> ...,
}
```

**前端契约：**

```ts
export interface PluginManifest {
  id: string;
  name: string;
  icon: string;
  keywords: string[];               // uTools 风格关键词触发
  routes: RouteRecord[];            // 懒加载
  settings?: SettingsSchema;
}
export function definePlugin(m: PluginManifest): PluginManifest;
```

**IPC 分发约定：** 前端调用 `invoke("plugin:<id>:<action>", payload)`，宿主 `app/ipc.rs` 按 `<id>` 路由到对应插件 handler。

### 4.4 按需加载

命令面板输入关键词 → 命中插件 → 懒加载该插件的 Vue 路由块（`defineAsyncComponent`）+ 调用后端 `on_activate`。未激活插件不占内存：Rust 端 trait 对象本身轻量，重资源（如 DB 连接）在 `on_activate` 时延迟初始化。

### 4.5 配置隔离

每插件独立配置命名空间 `<config_root>/plugins/<id>/`，避免不同插件配置混杂（如 FlyEnv 的环境管理与 filesweep 的规则互不干扰）。

---

## 5. 后续开发路线图

| 阶段 | 目标 | 关键交付 |
|---|---|---|
| **P0 清理** | 干净的 Tauri 单栈基线 | 旧 Go/旧前端删除、归档 tag/分支、`.gitignore`、文档归档、验证编译 |
| **P1 插件骨架** | 建立可扩展内核 | `Plugin` trait + 注册表 + IPC 分发 + 前端 `definePlugin` + 命令面板 shell（最小可用） |
| **P2 迁移 filesweep** | 现有功能成为首个内置插件 | 现有 commands/ + views 按插件边界重组，对外接口不变 |
| **P3 功能对等补齐** | 补齐 vs 旧 Go 设计 | 离线知识库 offline_db、import/export catalog、Rust CLI 子命令完善 |
| **P4 新插件** | 按优先级落地 | dev_env (FlyEnv) → env_vars → profile_backup (含软链接) → dedup (Czkawka) |
| **P5 工程化** | 可维护性 | 统一错误处理、日志、模块拆分、测试覆盖、CI、自动更新 |

**P2/P3 顺序**：先迁移（P2）再补齐（P3）。理由：先在插件骨架上重组现有代码，建立插件边界与范例，再做功能补齐时新功能天然按插件结构落地，避免补齐后再返工重组。

---

## 6. 头脑风暴建议（探讨项）

1. **dedup 与 filesweep 去重的关系**：Czkawka 是全盘重复扫描，filesweep 已有版本分组去重。建议 dedup 做成独立插件（更通用全盘扫描），filesweep 保留「软件版本管理」语义，两者互补而非合并。
2. **通用操作日志/回滚基础设施**：profile_backup 的软链接管理是 Windows 高风险操作。filesweep 已有 CSV 日志 + 回滚能力，建议抽到 `app/ops/` 作为通用基础设施，供所有破坏性操作插件复用（dry-run + 日志 + 回滚）。
3. **dev_env 定位**：多版本管理（Node/Python/Go 等）易与系统冲突，建议只做「版本切换 + PATH 管理」（nvm 风格），不做完整隔离环境（conda 风格），降低复杂度。
4. **命令面板优先级**：是 uTools 体验核心。建议 P1 就做最小可用版（全局快捷键唤起 + 关键词匹配 + 插件激活），后续再加历史/收藏/快捷动作。
5. **动态加载演进路径**：当前内置架构稳定后，若需第三方扩展，可评估「本地插件包」方案（manifest.json + frontend assets + 可选 wasm），类似 VSCode 扩展，作为 P4 之后的方向。

---

## 7. 验收标准

**P0 完成**当且仅当：
- [ ] `legacy/go-stack` tag 与 `archive/legacy-go-stack` 分支存在
- [ ] 上述删除清单全部从 main 移除
- [ ] `config/catalog.db` 不在版本控制，已在 `.gitignore`
- [ ] `.gitignore` 已补充
- [ ] `cargo check` 通过
- [ ] `npm run build` 通过
- [ ] 全仓 grep 无 `frontend/`、`cmd/`（旧 Go）、`go.mod` 残留引用
- [ ] 归档文档位于 `docs/archive/`
