# P0 旧栈清理 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 将 FileSweep 仓库清理为干净的 Tauri (Rust) + Vue3 单栈基线，删除全部旧 Go (Cobra+Gin) 与旧前端残留，归档历史以便回溯。

**Architecture:** 不涉及代码架构变更，纯清理操作。先归档（tag + 分支）保留历史，再分批删除旧源码/旧前端/产物/临时文件，更新 `.gitignore`，最后用 `cargo check` + `npm run build` 验证 Tauri 端无引用残留。

**Tech Stack:** Git, Tauri (Rust), Vue3 + Vite + TypeScript

**关联设计文档：** `docs/superpowers/specs/2026-06-18-plugin-platform-migration-design.md`

**关键事实（已核实）：**
- 新前端 `src/` 通过 `@` alias 引用自身，**不依赖** `frontend/`（已 grep 确认）
- Tauri 用 `../dist` 作为 frontendDist（根 dist，Tauri 前端产物，保留）
- `dist/filesweep.exe` 在工作区已删除（git status 的 `D`），仅暂存区残留，需 `git rm`
- `src-tauri/src/core/config.rs:24` 表明 `catalog.db` 路径指向用户配置目录，运行时生成
- `config/offline_db.sqlite` 是旧 Go 离线知识库种子数据，**保留**作为 P3 数据源（本次不删）
- `Users/Spence` 是误生成目录（应为绝对路径被当相对路径创建），删除

---

## 文件结构（本次涉及）

**删除：**
- Go 源：`main.go`, `embed.go`, `cmd/`, `internal/`, `tests/`
- Go 构建：`go.mod`, `go.sum`, `Makefile`
- 旧前端：`frontend/`
- 旧产物：`filesweep.exe`, `filesweep.exe~`, `catalog-all.csv`, `rustup-init.exe`
- 暂存残留：`dist/filesweep.exe`
- 误生成：`Users/`

**移动（归档）：**
- `FileSweep_DesignDoc_v1.0.md` → `docs/archive/`
- `原型.html` → `docs/archive/`
- `绿色软件识别.md` → `docs/archive/`

**修改：**
- `.gitignore`（补充忽略规则）
- `config/catalog.db`（git rm --cached，保留本地文件）

**保留不动：**
- `config/categories.yaml`, `config/config.yaml`, `config/rules.yaml`, `config/offline_db.sqlite`
- `dist/`（Tauri 前端产物）
- `src/`, `src-tauri/`, `index.html`, `package.json`, `vite.config.ts`, `tsconfig.json`, `tailwind.config.js`, `postcss.config.js`, `app-icon.png`
- `tauri-build.bat`, `tauri-dev.bat`

---

### Task 1: 归档（tag + 分支）

**目的：** 清理前固化历史，确保旧 Go 栈随时可回溯。

**Files:** 无（仅 git 元数据）

- [ ] **Step 1: 确认当前 HEAD 状态**

Run:
```bash
git log --oneline -1 && git status --short
```
Expected: HEAD 指向最新 commit（`64a75f3` 设计文档提交或其后），status 显示工作区有未跟踪/修改文件（正常）。

> 注意：归档应基于「清理前」的当前状态。工作区的未提交改动（如已删除的 `dist/filesweep.exe`）不进归档，归档只固化已提交的历史。

- [ ] **Step 2: 打 tag**

Run:
```bash
git tag legacy/go-stack
```
Expected: 无输出，tag 创建成功。

- [ ] **Step 3: 创建归档分支**

Run:
```bash
git branch archive/legacy-go-stack
```
Expected: 无输出，分支创建成功。

- [ ] **Step 4: 验证 tag 与分支**

Run:
```bash
git tag --list "legacy/*" && git branch --list "archive/*"
```
Expected:
```
legacy/go-stack
* main
  archive/legacy-go-stack
```
（`*` 标记当前分支为 main）

---

### Task 2: 归档历史文档

**目的：** 将旧设计文档与原型从根目录移入 `docs/archive/`，保持根目录整洁。

**Files:**
- Move: `FileSweep_DesignDoc_v1.0.md` → `docs/archive/FileSweep_DesignDoc_v1.0.md`
- Move: `原型.html` → `docs/archive/原型.html`
- Move: `绿色软件识别.md` → `docs/archive/绿色软件识别.md`

- [ ] **Step 1: 创建 docs/archive 目录并移动文件**

Run（Windows cmd）:
```bash
mkdir docs\archive
git mv "FileSweep_DesignDoc_v1.0.md" "docs\archive\FileSweep_DesignDoc_v1.0.md"
git mv "原型.html" "docs\archive\原型.html"
git mv "绿色软件识别.md" "docs\archive\绿色软件识别.md"
```
Expected: 三个 `git mv` 各输出一次重命名确认或无错误。

> 若某文件未被 git 跟踪（`git mv` 报 fatal），改用普通 `move` 后 `git add`：
> ```bash
> move "原型.html" "docs\archive\原型.html"
> git add "docs\archive\原型.html"
> ```

- [ ] **Step 2: 验证移动结果**

Run:
```bash
dir /b docs\archive
```
Expected:
```
FileSweep_DesignDoc_v1.0.md
原型.html
绿色软件识别.md
```

- [ ] **Step 3: 提交**

Run:
```bash
git add -A
git commit -m "docs: 归档旧设计文档与原型至 docs/archive"
```
Expected: 提交成功，显示 3 个文件重命名（rename）。

---

### Task 3: 删除旧 Go 源码与构建文件

**目的：** 移除 Go 后端实现，Tauri (Rust) 已完全取代。

**Files:** `main.go`, `embed.go`, `cmd/`, `internal/`, `tests/`, `go.mod`, `go.sum`, `Makefile`

- [ ] **Step 1: 删除 Go 源码目录**

Run:
```bash
git rm -r cmd internal tests
git rm main.go embed.go
```
Expected: 列出所有被删除的 `.go` 文件路径。

- [ ] **Step 2: 删除 Go 构建与依赖文件**

Run:
```bash
git rm go.mod go.sum Makefile
```
Expected: 三个文件删除确认。

> 注意：若 `git rm` 报「did not match any files」，说明该文件未被跟踪（可能已在工作区删除），用 `git add -A` 捕获删除即可。

- [ ] **Step 3: 验证 Go 残留**

Run:
```bash
dir /b /s *.go go.mod go.sum Makefile 2>nul
```
Expected: 无输出（或仅 `node_modules`/`target` 内的无关文件，本项目不应有）。

- [ ] **Step 4: 提交**

Run:
```bash
git commit -m "chore: 删除旧 Go 栈源码与构建文件

旧栈为 Go(Cobra CLI + Gin server)，已由 Tauri(Rust) 重构取代。
归档于 legacy/go-stack tag 与 archive/legacy-go-stack 分支。"
```
Expected: 提交成功，显示多个文件删除。

---

### Task 4: 删除旧前端 frontend/

**目的：** 移除旧 Vue3/naive-ui/axios 前端，新前端 `src/`（radix-vue/shadcn + Tauri invoke）已取代。

**Files:** `frontend/`（整目录）

**预检事实：** 已 grep 确认 `src/`、`vite.config.ts`、`package.json`、`tsconfig.json` 均不引用 `frontend/`。

- [ ] **Step 1: 删除 frontend 目录**

Run:
```bash
git rm -r frontend
```
Expected: 列出 `frontend/` 下所有文件删除确认。

> 若 `frontend/` 未被 git 跟踪，改用：
> ```bash
> rmdir /s /q frontend
> git add -A
> ```

- [ ] **Step 2: 复核新前端无引用**

Run:
```bash
findstr /s /i "frontend" src\*.ts src\*.vue vite.config.ts package.json tsconfig.json index.html 2>nul
```
Expected: 无输出（exit code 1，表示无匹配）。

- [ ] **Step 3: 提交**

Run:
```bash
git commit -m "chore: 删除旧前端 frontend/ 目录

旧前端为 Vue3+naive-ui+axios(HTTP 调用)，已由 Vue3+radix-vue+Tauri invoke 取代。"
```
Expected: 提交成功。

---

### Task 5: 删除旧产物与临时文件

**目的：** 清理构建产物、二进制、临时下载文件、误生成目录。

**Files:** `filesweep.exe`, `filesweep.exe~`, `catalog-all.csv`, `rustup-init.exe`, `dist/filesweep.exe`, `Users/`

- [ ] **Step 1: 删除旧二进制与产物**

Run:
```bash
git rm --cached dist/filesweep.exe 2>nul
del filesweep.exe filesweep.exe~ catalog-all.csv rustup-init.exe 2>nul
```
Expected: `git rm --cached` 移除暂存记录；`del` 删除工作区文件（不存在的文件 `2>nul` 静默）。

- [ ] **Step 2: 删除误生成的 Users 目录**

Run:
```bash
rmdir /s /q Users
```
Expected: 无输出，目录删除。`Users/Spence` 为误生成（绝对路径被当相对路径创建）。

- [ ] **Step 3: 验证清理**

Run:
```bash
dir /b filesweep.exe filesweep.exe~ catalog-all.csv rustup-init.exe Users 2>nul
```
Expected: 无输出（均已删除）。

- [ ] **Step 4: 提交**

Run:
```bash
git add -A
git commit -m "chore: 清理旧产物二进制与误生成目录

- 移除旧 Go 二进制 filesweep.exe(filesweep.exe~)
- 移除 catalog-all.csv、rustup-init.exe 临时文件
- 移除误生成的 Users/ 目录"
```
Expected: 提交成功。

---

### Task 6: 更新 .gitignore

**目的：** 补充忽略规则，防止运行时产物再次入库。

**Files:** `.gitignore`

- [ ] **Step 1: 写入完整 .gitignore**

用 Write 工具创建/覆盖 `.gitignore`，内容：

```gitignore
# Dependencies
node_modules

# Build output
dist
src-tauri/target

# Runtime / build artifacts
*.exe
*.exe~
*.csv

# Runtime database (path resolved at runtime, see src-tauri/src/core/config.rs)
config/catalog.db

# TypeScript build info
*.tsbuildinfo

# OS files
Thumbs.db
.DS_Store
```

- [ ] **Step 2: 将 catalog.db 移出版本控制（保留本地文件）**

Run:
```bash
git rm --cached config/catalog.db
```
Expected: `rm 'config/catalog.db'`，本地文件保留。

> 说明：`config/catalog.db` 是运行时数据库（`config.rs:24` 指向用户配置目录），仓库内的副本是历史快照。保留本地文件不影响运行，仅从 git 移除。

- [ ] **Step 3: 验证 catalog.db 已被忽略**

Run:
```bash
git status --short config/catalog.db
git check-ignore config/catalog.db
```
Expected: `git status` 无输出（已忽略）；`check-ignore` 输出 `config/catalog.db`。

- [ ] **Step 4: 提交**

Run:
```bash
git add .gitignore
git commit -m "chore: 补充 .gitignore 并将 catalog.db 移出版本控制

- 忽略 *.exe / *.csv / target / dist 产物
- catalog.db 为运行时数据库，路径在 core/config.rs 动态解析"
```
Expected: 提交成功。

---

### Task 7: 验证 Tauri 端完整性

**目的：** 确认清理未破坏 Tauri 构建，无残留引用。

- [ ] **Step 1: grep 全仓确认无旧栈引用**

Run:
```bash
findstr /s /i /m "go.mod gin cobra" *.md docs\*.md 2>nul
git ls-files | findstr /i "frontend cmd internal go.mod go.sum Makefile"
```
Expected: 第一条无 Go 相关引用（`go.mod` 等词仅在归档历史 doc 中可能残留，可接受）；第二条 `git ls-files` 无输出（已无跟踪的旧栈文件）。

- [ ] **Step 2: Rust 编译检查**

Run:
```bash
cd src-tauri && cargo check
```
Expected: `Finished` 且无 error。warning 可接受。

> 若无 cargo 环境，跳过此步并在验收记录里注明，改用 Step 3 前端构建 + Step 4 静态引用检查作为替代验证。

- [ ] **Step 3: 前端构建检查**

Run:
```bash
npm install
npm run build
```
Expected: `vite build` 成功生成 `dist/`，无 TS 错误（`vue-tsc` 通过）。

- [ ] **Step 4: 确认最终目录结构干净**

Run:
```bash
dir /b
```
Expected（核心项）:
```
.gitignore
app-icon.png
config
dist
docs
index.html
node_modules
package-lock.json
package.json
postcss.config.js
src
src-tauri
tauri-build.bat
tauri-dev.bat
tailwind.config.js
tsconfig.json
tsconfig.tsbuildinfo
vite.config.ts
```
**不应出现**：`main.go`, `embed.go`, `cmd`, `internal`, `tests`, `frontend`, `go.mod`, `go.sum`, `Makefile`, `filesweep.exe`, `Users`, `FileSweep_DesignDoc_v1.0.md`, `原型.html`, `绿色软件识别.md`, `catalog-all.csv`, `rustup-init.exe`。

---

### Task 8: 推送归档引用与最终状态记录

**目的：** 确保归档 tag/分支可被推送（若用远程），并记录清理完成状态。

**Files:** 无

- [ ] **Step 1: 查看最终提交历史**

Run:
```bash
git log --oneline -8
```
Expected: 看到本计划的若干 `chore:` / `docs:` 提交，叠加在设计文档提交之上。

- [ ] **Step 2: 确认归档引用存在**

Run:
```bash
git tag --list && git branch --list
```
Expected: `legacy/go-stack` tag 与 `archive/legacy-go-stack` 分支均在。

- [ ] **Step 3: （可选）若配置了远程，推送归档引用**

Run（仅在用户确认有远程且需要推送时）:
```bash
git push origin legacy/go-stack
git push origin archive/legacy-go-stack
```
Expected: 推送成功。若未配置远程，跳过此步。

- [ ] **Step 4: 向用户汇报清理结果**

汇报内容（在对话中输出）：
- 删除的旧栈项汇总（Go 源/旧前端/产物/临时文件数量）
- 归档位置（tag + 分支）
- `catalog.db` 移出版本控制
- `.gitignore` 补充项
- 验证结果（cargo check / npm run build 状态）
- 下一步建议：进入 P1（插件骨架）规划

---

## 验收标准（对齐设计文档第 7 节）

- [ ] `legacy/go-stack` tag 与 `archive/legacy-go-stack` 分支存在
- [ ] 删除清单全部从 main 移除
- [ ] `config/catalog.db` 不在版本控制，已在 `.gitignore`
- [ ] `.gitignore` 已补充（exe/csv/target/dist/tsbuildinfo）
- [ ] `cargo check` 通过（或因无环境跳过并记录）
- [ ] `npm run build` 通过
- [ ] 全仓 `git ls-files` 无 `frontend/`、`cmd/`、`internal/`、`go.mod` 残留
- [ ] 归档文档位于 `docs/archive/`

---

## 回滚指引

若清理后发现 Tauri 端有未发现的依赖导致构建失败：
```bash
git revert HEAD~5..HEAD   # 回滚本次清理的若干提交（按实际提交数调整）
# 或从归档检出特定文件
git checkout archive/legacy-go-stack -- <path>
```
归档分支与 tag 保证旧 Go 栈完整可恢复。
