# 目录分类两层方案设计

**日期**：2026-06-25
**状态**：已批准（grill-me 讨论成果）
**关联**：`docs/superpowers/specs/2026-06-22-product-definition.md`

---

## 1. 背景问题

当前扫描器只区分"app dir vs 集合目录"，无法识别以下真实场景：
- 代码项目（含 package.json / pom.xml / .git）
- Markdown 笔记（.md + 附件文件夹，图片占比可能 > .md）
- YAML 库（如 nuclei POC）
- CTF 题目（chall.py + 数据文件）
- 安全知识库（txt/html 混合）
- 样本集合（exe + rar 混合）
- 培训资料（docx/pptx 主导）
- 临时文件（无意义文件名）

这些场景需要**目录级别分类**（判断"这个目录是什么类型的资产"），而非文件级别分类。

---

## 2. 两层架构

### 层 1：目录类型识别（DirTypeClassifier）

在 find_app_roots **之前**执行。先识别已知类型的目录 → 整目录聚合保留。评分模型只处理 UNKNOWN 目录。

```
优先级 1: file_markers 检测（最快最准）
  - 含 package.json / go.mod / Cargo.toml / pom.xml / .git → CODE_PROJECT
  - 含 *.sln / *.vcxproj → CODE_PROJECT
  - 含 chall.py / flag.txt / solve.py / writeup.md → CTF_CHALLENGE

优先级 2: dir_name_keywords 匹配（dir_patterns 表）
  - 目录名含关键词 → 对应类型
  - 例："2024数字中国" 含"数字中国" → CTF_CHALLENGE
  - 例："wiki.tidesec" 含"wiki" → KNOWLEDGE_BASE
  - 例："xm样本" 含"样本" → SAMPLE_COLLECTION
  - 例："2023集团培训" 含"培训" → TRAINING_MATERIAL

优先级 3: 文件类型指纹（内置兜底）
  - NOTE_COLLECTION: ≥1 个 .md + (同名文件夹 或 assets 目录 或 md+图片合计 > 50%) + 无 exe
  - YAML_LIBRARY: .yaml/.yml 占比 > 60% + 无 exe
  - TEMP_FILES: 无意义文件名(1.txt / temp*.txt / 随机串) 占比 > 50%
  - DOC_COLLECTION: .docx/.pptx/.pdf/.xlsx 占比 > 60% + 无 exe

优先级 4: APP_DIR（现有评分模型）
  - 含 exe/dll/jar → 走 classify_dir 评分

优先级 5: UNKNOWN → 走 classify_dir 评分（保守保留）
```

### 层 2：处理策略

| 目录类型 | 策略 | 聚合 | 建议引擎 |
|---|---|---|---|
| CODE_PROJECT | 保留 | 整目录一条记录 | 不出现 |
| NOTE_COLLECTION | 保留 | 整目录一条记录 | 不出现 |
| YAML_LIBRARY | 保留 | 整目录一条记录 | 不出现 |
| CTF_CHALLENGE | 保留 | 整目录一条记录 | 不出现 |
| KNOWLEDGE_BASE | 保留 | 整目录一条记录 | 不出现 |
| SAMPLE_COLLECTION | 保留 | 整目录一条记录 | 不出现 |
| TRAINING_MATERIAL | 保留 | 整目录一条记录 | 不出现 |
| DOC_COLLECTION | 保留 | 整目录一条记录 | 不出现 |
| TEMP_FILES | 建议删除 | 不聚合 | 逐文件建议删除 |
| APP_DIR | 现有评分 | app root 聚合 | 走建议引擎 |
| UNKNOWN | 现有评分 | 按评分结果 | 走建议引擎 |

---

## 3. DB 表：dir_patterns（用户自定义目录模式）

```sql
CREATE TABLE dir_patterns (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    pattern_name TEXT NOT NULL,           -- "CTF题目" / "Markdown笔记" / "POC库"
    dir_type TEXT NOT NULL,               -- "CTF_CHALLENGE" / "NOTE_COLLECTION" 等
    dir_name_keywords TEXT DEFAULT '[]',  -- JSON ["CTF","数字中国","攻防","竞赛","赛"]
    file_markers TEXT DEFAULT '[]',       -- JSON ["chall.py","flag.txt","package.json"]
    file_type_ratio TEXT DEFAULT '{}',    -- JSON {"md+image": 0.5, "yaml": 0.6}
    same_name_dir INTEGER DEFAULT 0,      -- 是否要求同名文件夹
    require_no_exec INTEGER DEFAULT 1,    -- 是否要求无可执行文件
    action TEXT DEFAULT 'keep',           -- "keep" / "delete" / "app_dir"
    priority INTEGER DEFAULT 50,          -- 匹配优先级（小 = 高优先）
    enabled INTEGER DEFAULT 1
);
```

### 内置默认模式（首次初始化）

| pattern_name | dir_type | dir_name_keywords | file_markers | action | priority |
|---|---|---|---|---|---|
| 代码项目 | CODE_PROJECT | `[]` | `["package.json","go.mod","Cargo.toml","pom.xml",".git","Makefile"]` | keep | 10 |
| CTF题目 | CTF_CHALLENGE | `["CTF","数字中国","攻防","竞赛","赛","writeup","challenge","靶场"]` | `["chall.py","flag.txt","solve.py","writeup.md"]` | keep | 15 |
| 安全知识库 | KNOWLEDGE_BASE | `["wiki","knowledge","百科","tidewiki"]` | `[]` | keep | 20 |
| 样本集合 | SAMPLE_COLLECTION | `["样本","sample","malware"]` | `[]` | keep | 20 |
| 培训资料 | TRAINING_MATERIAL | `["培训","课程","通知","报告","规程","年审"]` | `[]` | keep | 20 |
| 漏洞资料 | VULN_MATERIAL | `["漏洞","上报","平台","poc","CVE","exploit"]` | `[]` | keep | 20 |
| Markdown笔记 | NOTE_COLLECTION | `["notes","note","blog","wiki","obsidian"]` | `[]` | keep | 25 |
| POC库 | YAML_LIBRARY | `["poc","nuclei","templates"]` | `[]` | keep | 25 |
| 临时文件 | TEMP_FILES | `["temp","tmp","cache"]` | `[]` | delete | 30 |

---

## 4. 临时文件判定（特殊处理）

临时文件的标志：
- 文件名无意义：纯数字（`1.txt`）、`temp`前缀（`temp*.txt`）、随机字符串（`a1b2c3.txt`）
- 文件名 ≤ 3 字符 + 常见后缀（`1.py`、`a.h`、`bb.c`）
- 文件名是 UUID/哈希（32 位十六进制）

```rust
fn is_meaningless_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("");
    // 纯数字
    if stem.chars().all(|c| c.is_ascii_digit()) && stem.len() <= 4 { return true; }
    // ≤ 2 字符
    if stem.len() <= 2 { return true; }
    // temp 前缀
    if stem.to_lowercase().starts_with("temp") { return true; }
    // 32 位十六进制（哈希/UUID 无连字符）
    if stem.len() == 32 && stem.chars().all(|c| c.is_ascii_hexdigit()) { return true; }
    false
}
```

TEMP_FILES 判定：无意义文件名占比 > 50%。

---

## 5. 匹配逻辑实现

```rust
/// 对目录树中每个目录，返回其类型（优先级从高到低匹配）
fn classify_dir_type(
    dir: &Path,
    node: &DirNode,
    patterns: &[DirPattern],
) -> DirType {
    let dir_name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();

    // 优先级 1: file_markers（dir_patterns 表）
    for p in patterns.iter().filter(|p| p.enabled && !p.file_markers.is_empty()) {
        if p.file_markers.iter().any(|m| node.files.iter().any(|f| f == m)) {
            return DirType::from(&p.dir_type);
        }
    }

    // 优先级 2: dir_name_keywords（dir_patterns 表）
    for p in patterns.iter().filter(|p| p.enabled && !p.dir_name_keywords.is_empty()) {
        if p.dir_name_keywords.iter().any(|kw| dir_name.contains(kw)) {
            return DirType::from(&p.dir_type);
        }
    }

    // 优先级 3: 文件类型指纹（内置兜底）
    // ... NOTE_COLLECTION / YAML_LIBRARY / TEMP_FILES / DOC_COLLECTION 检测 ...

    // 优先级 4: APP_DIR（交给评分模型）
    // 优先级 5: UNKNOWN（交给评分模型）
    DirType::Unknown
}
```

### 与现有扫描器的集成

```
collect_dir_tree
  → mark_subtree_stats（现有）
  → classify_dir_type（新增层 1）→ 已知类型目录标记保留 + 聚合
  → find_app_roots（现有层 2）→ 仅对 UNKNOWN 目录运行评分模型
  → scan_files（现有）→ APP_DIR/UNKNOWN 目录的文件扫描
```

---

## 6. 扫描完成后目录类型展示

FileRecord 加 `dir_type` 字段（或复用 `app_dir_reason`）：

| 字段 | 值 | 含义 |
|---|---|---|
| is_app_dir | true | 聚合目录（所有类型） |
| app_dir_reason | "exe-app" / "CODE_PROJECT" / "CTF_CHALLENGE" / ... | 目录类型 |

前端 FileListView 显示类型 Badge（"代码项目"/"CTF题目"/"笔记"等），可按类型筛选。

---

## 7. 实现任务顺序（下次会话）

1. DB migration: `dir_patterns` 表 + 默认数据
2. db/config.rs: dir_patterns CRUD 方法
3. core/dir_classifier.rs: classify_dir_type 实现
4. scanner.rs: 集成到四阶段扫描（mark 后、find_app_roots 前）
5. is_meaningless_name 实现
6. actions.rs: config:patterns:* CRUD
7. 前端 ConfigView 加"目录模式"tab
8. 前端 FileListView 类型 Badge + 筛选
9. 前端 SuggestionPanel：TEMP_FILES 显示删除建议
