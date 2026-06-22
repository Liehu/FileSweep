# 配置 DB 化 + 软件根路径优化设计

**日期**：2026-06-22
**状态**：已批准

---

## 1. 目标

1. 静态 YAML 配置迁移到 DB 表（rules.yaml + categories.yaml + exclude 规则）
2. 软件安装根路径表（software_roots），扫描时一级目录直接识别为 app dir
3. AI API key 走环境变量（不入库）
4. 独立配置页面（route: /config）

---

## 2. DB 表设计（4 张新表）

### `software_roots`（软件安装根路径）

```sql
CREATE TABLE software_roots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL UNIQUE,
    enabled INTEGER DEFAULT 1,
    display_name TEXT DEFAULT ''
);
```

默认数据（首次初始化）：
- `C:\Program Files`
- `C:\Program Files (x86)`
- `D:\Program Files`（如存在）
- `E:\Program Files`（如存在）
- `%LOCALAPPDATA%\Programs`

### `category_rules`（分类规则，替代 rules.yaml）

```sql
CREATE TABLE category_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    target_path TEXT DEFAULT '',
    extensions TEXT DEFAULT '[]',      -- JSON ["\\.exe","\\.msi"]
    name_keywords TEXT DEFAULT '[]',   -- JSON ["setup","install"]
    app_dir_only INTEGER DEFAULT 0,
    priority INTEGER DEFAULT 0,        -- 高优先匹配
    enabled INTEGER DEFAULT 1
);
```

### `func_categories`（功能分类，替代 categories.yaml）

```sql
CREATE TABLE func_categories (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    keywords TEXT DEFAULT '[]',        -- JSON ["EasyBCD","rEFInd"]
    parent TEXT DEFAULT '',
    enabled INTEGER DEFAULT 1
);
```

### `exclude_rules`（排除规则，统一表）

```sql
CREATE TABLE exclude_rules (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_type TEXT NOT NULL,           -- "dir" / "ext" / "name"
    pattern TEXT NOT NULL,
    enabled INTEGER DEFAULT 1
);
```

---

## 3. 数据迁移（首次启动）

首次启动检测旧 YAML 文件存在时，导入到 DB：
1. `rules.yaml` → `category_rules` 表
2. `categories.yaml` → `func_categories` 表
3. `config.yaml` 的 exclude_dirs/names/exts → `exclude_rules` 表
4. 导入后 YAML 文件保留（备份），但不再读取

后续所有配置读写都走 DB。

---

## 4. AI API Key（环境变量）

不入库，从环境变量读取：
- `FILESWEEP_AI_API_KEY` — API key
- `FILESWEEP_AI_PROVIDER` — claude/openai/ollama/offline
- `FILESWEEP_AI_BASE_URL` — 自定义 API 端点（可选）

settings 表只存 provider 选择和模型名等非敏感配置。

---

## 5. 扫描策略

### 软件安装根路径（software_roots 表的路径）

```
扫描 software_roots 的每个 enabled 路径：
  1. 只 read_dir 一级条目（不递归）
  2. 一级子目录 → 直接 app dir（聚合记录，compute_dir_size + exe 列表）
  3. 根目录散装 exe/jar → 也作为 app dir（单文件）
  4. 不扫描子目录内部文件（不 hash、不入库）
  5. 排除规则（exclude_rules 表）适用
```

**性能**：秒级（只 read_dir 一层）。

### 普通目录（用户手动扫描）

保持现有四阶段架构（collect_dir_tree → mark → find_roots → scan_files）+ 评分模型。

### 扫描入口判断

scan:start 时检查扫描路径是否在 software_roots 表：
- 是 → `scan_software_roots()`（简化策略）
- 否 → `scanner.scan()`（评分模型）

---

## 6. 后端 API（新增 actions）

### software_roots CRUD

- `config:roots:list` — 列出所有软件根路径
- `config:roots:add` — 新增（path + display_name）
- `config:roots:update` — 更新（id + path/display_name/enabled）
- `config:roots:delete` — 删除（id）

### category_rules CRUD

- `config:categories:list`
- `config:categories:add`
- `config:categories:update`
- `config:categories:delete`

### func_categories CRUD

- `config:func_categories:list`
- `config:func_categories:add`
- `config:func_categories:update`
- `config:func_categories:delete`

### exclude_rules CRUD

- `config:exclude:list`
- `config:exclude:add`
- `config:exclude:update`
- `config:exclude:delete`

所有 CRUD 走 plugin_invoke，spawn_blocking 避免 lock 竞争。

---

## 7. 前端配置页面

### 路由

`/config`（独立页面，侧栏「工具」组新增入口）

### 结构

四个 tab（shadcn Tabs 组件）：

1. **软件安装路径**
   - 表格：path / display_name / enabled（开关）
   - 操作：新增 / 编辑 / 删除 / 启用切换

2. **分类规则**
   - 表格：name / target_path / extensions / keywords / app_dir_only / priority
   - 编辑弹窗：extensions 和 keywords 用 tag input（逗号分隔）
   - 操作：新增 / 编辑 / 删除

3. **功能分类**
   - 表格：name / parent / keywords
   - 编辑弹窗：keywords 用 tag input
   - 操作：新增 / 编辑 / 删除

4. **排除规则**
   - 表格：rule_type（下拉 dir/ext/name）/ pattern / enabled
   - 操作：新增 / 删除 / 启用切换

---

## 8. 与现有代码的关系

| 现有 | 改动 |
|---|---|
| `rules.yaml` | 迁移到 category_rules 表（首次导入） |
| `categories.yaml` | 迁移到 func_categories 表（首次导入） |
| `config.yaml` 的 exclude | 迁移到 exclude_rules 表 |
| `Classifier::new(rules_path)` | 改为 `Classifier::from_db(&conn)` |
| `scanner.scan()` | 不改（普通目录仍用） |
| 新增 `scan_software_roots()` | 简化策略扫描 |
| scan:start | 检查路径是否 software_roots |
| config.rs 的 AI 配置 | 改为读环境变量 |
| settings.ts | 配置相关 store 新增 |

---

## 9. 实现任务顺序

1. DB migration：4 张新表 + 默认数据 + YAML 导入逻辑
2. db CRUD 方法：4 张表各 CRUD
3. actions：config:* 系列 actions
4. Classifier::from_db：从 DB 读分类规则
5. scan_software_roots：简化扫描策略
6. scan:start 判断逻辑
7. AI API key 环境变量
8. 前端配置页面（4 tab）
9. 侧栏入口
10. 清理旧诊断日志
