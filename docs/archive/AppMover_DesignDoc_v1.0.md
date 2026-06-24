# AppMover（软件迁移）

AppData / Program Files 目录迁移与环境备份工具

设计文档 · v1.0

| **技术栈** | Tauri 2 + Rust + Vue 3 + Pinia |
| :--- | :--- |
| **目标平台** | Windows（依赖 Junction / 注册表 / Uninstall 表） |
| **插件形态** | FileSweep 框架下的并立 UI 插件 |
| **运行模式** | 管理员态 GUI + 托盘常驻 + 轮询监控 |
| **文档版本** | v1.0 2026-06 |

> 本文档是 grill-me 设计评审会议的结论沉淀。所有决策点（"取舍"小节）均经过反向诘问，可追溯。

---

## 1. 项目概述

### 1.1 背景与目标

Windows 用户长期使用后，`AppData`、`Program Files` 等系统盘目录被大量软件数据占满。常见痛点：

- C 盘空间告急，但软件数据散落在 `AppData\Roaming`、`AppData\Local`、`Program Files` 各处，不知哪些可迁、哪些是系统默认不能动。
- 手动剪切移动后软件打不开（注册表/快捷方式/服务路径全断），只能重装。
- 软件装了卸、卸了装，残留目录无人清理，越积越多。
- 环境变量（尤其 `PATH`）被各种软件反复改写，出了问题无法回滚到已知好状态。

AppMover 解决上述问题，提供：

- **识别**：自动区分"系统默认目录"与"软件/用户数据目录"，只把后者列为候选。
- **安全迁移**：把候选目录整体搬到目标盘，并在原位建立 Junction（目录联接），让原应用无感继续使用。
- **占用解锁**：迁移前检测并关闭占用目录的进程（含被 explorer 加载的 shell 扩展 DLL）。
- **动态监控**：轮询监控根，发现新增/卸载残留目录时提醒。
- **环境备份**：备份/恢复用户与系统环境变量。

> **核心设计原则**
>
> ① 不碰系统默认目录（强白名单 ∪ 基线）。② 迁移用"复制→校验→建链接→删源"，中断时 C: 原件完整无损。③ 注册表只做只读展示和已勾选确认的写入，绝不静默改写软件注册表。

### 1.2 与 FileSweep 的关系

| 维度 | FileSweep | AppMover |
| :--- | :--- | :--- |
| 处理对象 | 文件 | 目录（整体软件数据） |
| 动作语义 | 识别垃圾/归类 → 清理或整理 | 识别非系统目录 → 搬家 |
| 迁移含义 | 把识别出的文件挪到归档区 | 把整个软件目录搬到 D: + 建 Junction |
| 扫描模型 | 按扩展名/分类扫文件 | 按软件归属扫一级子目录 |

两者职责不重叠，UI 上明确区隔"文件整理 vs 软件搬家"，正常不同时使用。

### 1.3 使用场景

| **场景** | **用户行为** | **工具响应** |
| :--- | :--- | :--- |
| C 盘瘦身 | 配置源根→目标根映射，扫描候选，勾选执行 | 复制到 D: + 建 Junction，应用无感 |
| 全新装机建立基线 | 在纯净 VM 扫描，导出基线文件，导入本机 | 基线目录加入保护集，永不进入候选 |
| 软件残留清理 | 启动监控，查看 resident 事件 | 列出卸载残留目录，确认后处理 |
| 环境变量回滚 | 备份当前环境变量，出问题时恢复 | 按时间组恢复到任意备份点 |
| 迁移失败续传 | 历史页对 failed 任务点重试 | 跳过已复制文件，续传剩余 |

---

## 2. 功能模块总览

| **模块** | **优先级** | **类型** | **描述** |
| :--- | :--- | :--- | :--- |
| 候选识别 | **P0** | 核心 | 强白名单 ∪ 基线 → 保护集；其余为候选迁移集 |
| 基线管理 | **P0** | 核心 | 基线文件导入 / 首次扫描作基线 / 保护集 CRUD |
| 目标根映射 | **P0** | 核心 | 源根 → 目标根配置，迁移时自动解析目标路径 |
| 迁移执行（方案 P） | **P0** | 核心 | 复制→校验→建 Junction→删源，带 checkpoint 续传 |
| 占用检测与关闭 | **P0** | 核心 | 进程驱动杀进程 → 模块反查重启 explorer → 标记手动 |
| 迁移历史 | **P0** | 核心 | 作业记录 + 失败续传 |
| 目录监控 | **P1** | 扩展 | 轮询一级目录增删，new/resident 事件提醒 |
| 环境变量备份恢复 | **P1** | 扩展 | 用户/系统环境变量按时间组备份恢复 |
| 已安装程序只读展示 | **P1** | 扩展 | Uninstall 注册表枚举（只读，不做恢复） |
| 软件描述 | **P1** | 扩展 | 预置映射 + AI 补全 |
| 托盘与自启 | **P2** | 扩展 | 系统托盘角标 + 开机自启 |

---

## 3. 关键决策记录（grill 结论）

> 以下每条都经过反向诘问，记录"选了什么"和"为什么不选另一条"。

### 3.1 迁移对象范围

**决策**：只迁"目录"，不迁散文件；仅当前用户；`ProgramData` 不迁；监控根 = `AppData\{Roaming,Local,LocalLow}` + `Program Files` + `Program Files (x86)`。

**否决**：
- 迁散文件 → 单文件 symlink 需管理员/开发者模式，且配置文件链接易被软件当"不可写"报错，收益小风险高。
- 多用户 → 复杂度倍增，MVP 只迁当前用户。
- ProgramData → 全机器共享，迁移影响所有用户，风险高。

### 3.2 系统默认识别（保护集）

**决策**：保护集 = 强白名单 ∪ 基线 ∪ 用户手动加入。候选集 = 监控根下"非保护集"的一级子目录。

**强白名单**（33 个，硬编码不可删）：`Microsoft`、`Packages`、`Common Files`、`WindowsApps`、`Windows Defender`、`Microsoft.NET`、`Desktop`/`Documents`/`Downloads` 等用户库目录、`OneDrive` 等。

**否决**：
- 只靠纯净 VM 基线 → 系统更新会新增默认目录，基线陈旧会误判（把系统更新产生的新目录当成软件目录）。强白名单兜底。
- 做软件识别（判断"这是哪个软件"）→ 与迁移目标无关，徒增复杂度。只识别"是不是系统默认"即可。

### 3.3 基线语义

**决策**：基线**仅用于**"识别系统默认目录"，不做快照恢复，无版本/时间戳/回滚链。

**否决**：
- 快照恢复 → 本软件不做数据回滚，基线就是"一个目录集合"的导入/导出，无需重型版本管理。

### 3.4 迁移机制

**决策**：方案 X（真身在 D:，C: 原位建 Junction）+ 方案 P（复制→校验→建链接→删源）。

**方案 P 四段式**：
1. **复制**：逐文件复制到 D:，每个文件复制完打 checkpoint（持久化到 DB），中断可续传。
2. **校验**：逐文件比对大小（关键场景可加 hash）。
3. **建链接**：C: 原件重命名为 `.amold_backup` → 在原位建 Junction 指向 D: → 删 `.amold_backup`。
4. **完成**：记录 job 为 done。

**Junction 而非 symlink/hardlink**：Junction 只需本机卷、不需管理员权限、跨本地盘 OK，最适合此场景。

**否决**：
- 方案 Q（直接 move + 日志续传）→ 跨卷 move 内部是"复制+逐个删源"，中断时单大文件会损坏（传到 60% 断电，D: 残缺 + C: 已删）。方案 P 中断时 C: 原件完整，无损重跑。
- 移动 + 改注册表/快捷方式/服务路径 → 极易翻车，且与"Junction 让软件无感"路线冲突（既然 junction 已让路径无差异，改注册表就是多余且危险）。

### 3.5 占用检测与关闭

**决策**：三级关闭策略。
1. **进程驱动**：枚举"exe 路径落在待迁目录下"的进程 → 优雅关闭（WM_CLOSE，等 5s）→ 强杀（TerminateProcess，等 2s）。
2. **模块反查**：对 explorer/dllhost 等外壳进程，枚举其加载模块（Module32），若加载了目录内 DLL → **自动重启 explorer**（shell 扩展 DLL 随 explorer 重启卸载）。
3. **标记手动**：仍失败（如受 PPL 保护的杀软）→ 标记该目录"不可安全迁移"，从队列剔除。

**执行前**：弹窗 + 10s 倒计时确认（给用户存盘时间），默认提示"已知会丢失未保存数据"。

**否决**：
- 只靠"移动失败才发现占用" → 此时复制阶段已被锁，卡住。必须迁移前预检。
- "引用驱动只做提示" → 漏掉 explorer 加载的 DLL 场景（右键菜单 shell 扩展），这正是需求点名要解决的（WPS/解压软件）。模块反查 + 重启 explorer 是正解。

### 3.6 运行形态

**决策**：方案 b —— 管理员态 GUI + 开机自启 + 托盘常驻 + 轮询监控。

**否决**：
- 方案 c（Windows 服务 + 前台 UI）→ 工程量最大、需 sidecar、Tauri 非为此设计。托盘常驻 + 后台轮询已满足"动态监控 + 角标提醒"，不值得背上整套服务架构。

### 3.7 监控

**决策**：轮询（非实时 ReadDirectoryChangesW），周期可配（15min/30min/1h/1day），只关心"一级目录列表的增删"，不递归内容。

**否决**：
- 实时监听 → AppData 高频变动，回调风暴。轮询延迟对"提醒"场景可接受。
- 递归内容监控 → 文件级变化不该触发迁移提醒，无意义。

### 3.8 注册表操作边界

**决策**：

| 范围 | 动作 | 风险 |
| :--- | :--- | :--- |
| 用户环境变量 `HKCU\Environment` | 备份/恢复 | 低 ✅ |
| 系统环境变量 `HKLM\...\Session Manager\Environment` | 备份/恢复（需管理员） | 中 ✅ |
| Uninstall 注册表 | **只读展示**，不备份不恢复 | — ✅ |
| 单个软件注册表子树（如 `HKLM\SOFTWARE\Adobe`） | **不做** | 极高 ❌ |
| 迁移后改写注册表路径引用 | **不做**（Junction 已让路径无差异） | — ❌ |

**否决**：备份/恢复"软件注册表"——软件注册表散落多处（CLSID/TypeLib/App Paths/Services），备份不全，恢复可能盖掉系统更新，是炸弹。

### 3.9 可逆性

**决策**：方案 B —— 不提供一键还原，但记完整迁移历史日志（原路径/新路径/时间/状态）。

### 3.10 软件描述

**决策**：预置目录名→软件名映射表（30 条）+ AI 回退接口（预留，MVP 返回 None）。

---

## 4. 技术架构

### 4.1 模块结构

```
src-tauri/src/plugins/appmover/
├── mod.rs              # 插件定义 + 5 feature + 27 action 声明
├── actions.rs          # IPC 分发（spawn_blocking 包装所有 DB/IO）
├── models.rs           # 数据结构（IPC/DB 共用）
├── baseline.rs         # 强白名单(33) + 基线导入 + 保护集
├── identify.rs         # 候选集识别（只收目录、过滤保护集）
├── describe.rs         # 预置映射(30) + DB 覆盖 + AI 接口
├── migrate/
│   ├── planner.rs      # 目标解析 + 空间预检(×1.1) + 锁定检测
│   ├── copier.rs       # 方案 P + checkpoint 续传 + 进度 emit
│   ├── locker.rs       # 进程枚举(tlhelp32) + 外壳模块反查(Module32)
│   └── killer.rs       # 三级关闭
├── monitor.rs          # 轮询 + 一级目录增删 + new/resident 状态机
├── envvar.rs           # reg query/add 备份恢复（保留 REG_EXPAND_SZ）
└── uninstall.rs        # 只读枚举 HKLM/HKLM-WOW64/HKCU
```

### 4.2 DB Schema（追加到现有 SQLite）

```sql
-- 迁移目标根映射
am_target_map (source_root PK, target_root)

-- 保护集
am_protected (path PK, source)  -- source: hardcoded | baseline | user

-- 迁移作业（方案 P 的 checkpoint 也写这）
am_migrate_jobs (id PK, source_path, target_path, status,
                 checkpoint JSON, file_count, copied_count, total_bytes,
                 started_at, finished_at, error)
-- status: planned | copying | verifying | linking | done | failed | manual

-- 监控快照
am_monitor_snapshot (watch_root, dir_name, first_seen_at, last_seen_at, state, PK(watch_root,dir_name))
-- state: new | resident | normal | gone

-- 环境变量备份
am_env_backup (id PK, scope, key, value, backed_up_at)
-- value 前缀 [E] 标记 REG_EXPAND_SZ

-- 软件描述映射
am_describe_map (dir_name PK, software_name, description, source)
```

### 4.3 迁移作业状态机

```
planned ──预检空间──▶ copying(checkpoint 持久化每文件)
                         │ 失败/中断
                         └─▶ failed（C: 原件完整，可 retry 续传）
copying ──▶ verifying(逐文件 size 比对) ──失败──▶ failed
verifying ──▶ linking(mklink /J C:→ D:) ──失败──▶ failed（D: 完整、C: 原件在，安全）
linking ──▶ deleting(删 C: 原件) ──失败──▶ manual（C: 有 junction + 原件并存，提示手动）
deleting ──▶ done
```

### 4.4 占用检测与关闭流程

```
scan_locks(dir)
  ├─ 枚举"exe 落在 dir 下"的进程 → blocking_processes（进程驱动可杀）
  ├─ 枚举外壳进程(explorer/dllhost)加载的模块 → 若含 dir 内 DLL → need_explorer_restart
  └─ 输出 LockReport

kill_locks(LockReport, dir)
  ├─ 优雅关闭(WM_CLOSE, 5s) ─▶ 强杀(TerminateProcess, 2s)
  ├─ 若 need_explorer_restart → 重启 explorer（taskkill /F explorer.exe && start explorer.exe）
  ├─ 复检 scan_locks(dir)
  └─ safe ? "可安全迁移" : 标记 manual

（执行 kill_locks 前弹窗 + 10s 倒计时确认）
```

### 4.5 监控状态机

```
每轮 poll_once：
  对每个监控根 read_dir 取一级目录集合 current
  加载 DB 旧快照 prev
  current - prev = 新增 → state='new'（候选迁移），发事件
  prev - current = 消失 → state='gone'（已清理，仅更新时间）
  current 中且不在 installed_software_dirs 的 → state='resident'（卸载残留），发事件
  用户 dismiss → state='normal'（不再提醒）
```

---

## 5. IPC 命令清单

| 分类 | 命令 | 说明 |
| :--- | :--- | :--- |
| 识别 | `am:scan_candidates` | 扫描候选迁移目录（已过滤保护集 + 补描述） |
| | `am:describe` / `am:describe_update` / `am:list_describe` | 描述查询/更新/列表 |
| 基线 | `am:import_baseline` | 文件或目录名数组导入基线 |
| | `am:set_first_scan_as_baseline` | 当前监控根一级目录作基线 |
| | `am:get_protected` / `am:add_protected` / `am:remove_protected` | 保护集 CRUD |
| 映射 | `am:get_target_map` / `am:set_target_map` / `am:remove_target_map` | 目标根映射 CRUD |
| 迁移 | `am:plan_migration` | 规划（目标解析+空间预检+锁定检测） |
| | `am:scan_locks` / `am:kill_locks` | 占用检测/三级关闭 |
| | `am:execute_migration` | 方案 P 执行（emit `am:migrate_progress` 事件） |
| | `am:retry_migration` / `am:list_jobs` | 续传/历史 |
| 监控 | `am:start_monitor` / `am:stop_monitor` | 启停 |
| | `am:get_monitor_events` / `am:dismiss_event` | 事件列表/确认 |
| 环境 | `am:backup_env` / `am:restore_env` / `am:list_env_backups` | 环境变量备份恢复 |
| | `am:list_installed` | 已安装程序只读 |

---

## 6. 前端结构

```
src/plugins/appmover/
├── index.ts / nav.ts / routes.ts   # 自动被 _registry.ts 收集
├── stores/appmover.ts              # 统一 store，封装全部 27 个 IPC
└── views/
    ├── MigrateView.vue    # 目标映射 + 候选列表 + 规划 + 关闭占用 + 倒计时 + 方案 P 执行
    ├── MonitorView.vue    # 启停轮询 + 待处理事件
    ├── HistoryView.vue    # 迁移历史 + 失败续传
    ├── EnvVarView.vue     # 环境变量备份恢复 + 已安装程序只读
    └── BaselineView.vue   # 基线导入 + 保护集 CRUD
```

---

## 7. 风险清单

| 风险 | 缓解 |
| :--- | :--- |
| 方案 P 双倍空间，D: 撑爆 | 迁移前 `GetDiskFreeSpaceExW` 预检（size × 1.1 ≤ free），不够直接拒 |
| Junction 后少数软件硬编码 C: 绝对路径仍失效 | 文档明确"junction 保证 90% 软件无感，硬编码路径软件需重装"；不强行改注册表 |
| 重启 explorer 影响用户拖拽/复制 | 仅在用户授权关闭后执行，弹窗显式提示 |
| 轮询误报（系统更新新增目录被判 new） | "new" 需用户确认才迁移；强白名单兜底 |
| 基线陈旧（系统更新产生新默认目录） | 强白名单兜底；支持用户"把当前 new 目录加入保护集" |
| 管理员态运行导致目录权限错乱 | 迁移后用 icacls 显式还原 D: 副本 owner 为当前用户（待实现） |
| 环境变量恢复后旧进程不感知 | 恢复后广播 WM_SETTINGCHANGE；文档提示"新进程才感知" |

---

## 8. 实施进度

### 8.1 已完成（MVP / P0）

- [x] 后端：全部 9 个 Rust 模块 + 6 张 DB 表 + 27 个 IPC 命令
- [x] 候选识别（强白名单 ∪ 基线）
- [x] 方案 P 迁移（复制→校验→Junction→删源 + checkpoint 续传）
- [x] 占用检测（进程枚举 + 外壳模块反查）+ 三级关闭
- [x] 迁移历史 + 失败续传
- [x] 轮询监控 + new/resident 事件
- [x] 环境变量备份恢复（reg query/add，保留 REG_EXPAND_SZ）
- [x] 已安装程序只读展示
- [x] 软件描述（预置映射 30 条 + DB 覆盖 + AI 接口预留）
- [x] 前端 5 个 View + store + 路由/导航注册
- [x] `cargo check` 通过 + 11 个单测通过
- [x] `vue-tsc` 类型检查通过 + `vite build` 生产构建通过

### 8.2 待办

- [ ] **P1** 托盘图标 + 角标状态机（依赖已加：tauri-plugin-notification / tray-icon）
- [ ] **P1** 开机自启（依赖已加：tauri-plugin-autostart）
- [ ] **P2** AI 描述接入（`describe::describe_with_ai` 留了接口，接 `crate::ai`）
- [ ] **P3** 迁移进度断点续传 UI 打磨（当前后端已支持 checkpoint，前端可显示已复制/总数）
- [ ] **P3** 迁移后 icacls 还原 D: 副本 owner
- [ ] 测试：方案 P 端到端在真实大目录（Adobe/VS 量级）的稳定性测试

---

## 附录 A：纯净 VM 基线导出指南

1. 在 Hyper-V / VMware 新建 Windows VM，**只装系统，不装任何第三方软件**。
2. 以目标用户登录，运行本软件 → 基线管理 → "首次扫描作基线"。
3. 导出保护集（source='baseline' 的条目）为文本文件（每行一个目录名）。
4. 在实际机器上 → 基线管理 → "导入基线文件"，选择该文件。
5. 强白名单（hardcoded）始终自动生效，无需导入。

> 基线仅用于"识别系统默认目录"，不做快照恢复。系统大版本更新后建议重新导出基线。

## 附录 B：迁移失败排查

| 现象 | 原因 | 处理 |
| :--- | :--- | :--- |
| 规划时 `space_ok=false` | 目标盘剩余 < size × 1.1 | 清理目标盘或换目标根 |
| 规划时 `locks.safe=false` | 目录被进程/DLL 占用 | 点"关闭占用进程"，或手动关软件 |
| 复制阶段 failed | 单文件复制失败（权限/占用） | C: 原件完整，历史页点"重试"续传 |
| linking 阶段 failed | 建 Junction 失败 | D: 已完整，C: 原件也在，安全重试 |
| deleting 阶段 manual | 删 C: 原件失败（C: 同时有 junction + 原件） | 手动删 `XXX.amold_backup` |
