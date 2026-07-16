# FileSweep 迭代开发计划（基于真实数据反馈）

**日期**：2026-07-02
**背景**：基于 45 万行 / D+E 盘 / 含 89TB OneDrive 占位文件的真实数据处理经验反馈
**目标**：补齐核心扫描引擎的关键缺口，让"持续检测 + 定期提醒"这个核心卖点在 TB 级数据下名副其实

---

## 0. 现状对照（反馈 vs 代码）

| 反馈点 | 当前代码现状 | 缺口 |
|---|---|---|
| **增量监听** | 无 notify 依赖，只有全量扫描 | 🔴 完全缺失 |
| **OneDrive 占位文件** | 无 reparse point 检测，partial hash 会触发云端下载 | 🔴 高风险 |
| **哈希分层策略** | 有 partial/metadata 分档（惰性），但无 size 分组，用 SHA256 | 🟡 可优化 |
| **默认排除噪音目录** | 只有系统目录，无 node_modules/target/\_\_pycache\_\_ | 🟡 性能前提 |
| **目录级分类** | ✅ 已有 dir_classifier（dir_patterns） | ✅ 基本完成 |
| **aho-corasick 匹配** | 线性匹配（规则量小，暂够用） | 🟢 可选优化 |
| **离线知识库** | offline_knowledge.db 是 0 字节空文件 | 🟡 无预置数据 |
| **quarantine 隔离** | 走系统回收站 + ~/.filesweep_trash 兜底 | 🟢 可改进 |

---

## P0：扫描引擎健壮性（不修就会翻车）

### P0-1：默认排除噪音目录清单（最快见效）

**问题**：当前 exclude_rules 默认只有 Windows/$Recycle.Bin 等系统目录，没有 node_modules/target/__pycache__/.venv/build/dist 等开发噪音目录。FileSweep 自己的 src-tauri/target/debug/deps 就有 1.7 万文件。

**改动**：`migrations.rs` 的 `init_default_config` → exclude_rules 默认数据增加：
```
node_modules, target, __pycache__, .venv, venv, build, dist, .next, .nuxt,
.obj, bin, Debug, Release（C++ 编译输出）, .gradle, .idea, .vscode（IDE 缓存）
```

**影响**：migrations.rs 默认数据 + 已有 DB 需补录（增量迁移逻辑）
**风险**：低。注意排除是"子串匹配 local_path"（见 scan.rs:103-108），要确保不误排除用户真实目录

### P0-2：OneDrive / 云占位文件检测（防云端下载事故）

**问题**：OneDrive "文件随需下载"的占位符是 reparse point，本地只有几 KB 指针。当前 `process_normal_file` 对内容文件会 `compute_partial_hash`（读头尾 4KB），这会触发 OneDrive 客户端把云端文件**强制下载到本地**——轻则扫描巨慢，重则写爆网盘配额和本地磁盘。

**改动**：scanner.rs 的 `collect_normal_files` + `process_normal_file` 增加占位文件检测：
- Windows: `GetFileAttributesW` 检查 `FILE_ATTRIBUTE_OFFLINE`（0x1000）/ `FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS`（0x400000）
- 检测到云端占位文件 → **跳过内容哈希**，仅用元数据 hash（size + mtime + path）参与去重
- Cargo.toml 加 windows crate 的 `Win32_Storage_FileSystem` feature（或用 std::fs::metadata 的 reparse point 检测）

**影响**：scanner.rs（collect_normal_files + process_normal_file）+ Cargo.toml
**风险**：中。需要 Windows API 调用，注意 32/64 位兼容；非 Windows 平台无此问题

### P0-3：哈希分层策略对齐（size 分组 + blake3）

**问题**：当前扫描阶段对所有文件无差别进 hash_file_list 并发处理，没有"先 size 分组，组内 >1 才算哈希"的优化。虽然 partial hash 已只读 8KB，但 45 万文件的元数据读取 + 调度开销仍大。

**改动**：
1. `collect_normal_files` 后增加 size 分组阶段：相同 size 的文件才进哈希队列（唯一 size 直接用 metadata hash）
2. 哈希算法从 SHA256 换 **blake3**（比 SHA 系快 5-10x，Cargo.toml 加 blake3 依赖）
3. 保留 SHA256 用于需要精确全文的少数场景（向后兼容 compute_hash）

**影响**：scanner.rs（hash_file_list 改造）+ dedup.rs（hash 比对）+ Cargo.toml
**风险**：中。blake3 hash 值与现有 SHA256 不兼容，需要 DB schema 或 hash 前缀标记区分（如 `b3:xxx` vs `sha:xxx`）

### P0-4：增量监听 / 变更守护（核心卖点支撑）

**问题**：无 notify 依赖，"持续检测目录变更"只能靠全量重扫，TB 级数据下不可行。

**方案**（分两阶段）：
- **阶段 A（快速 diff 快照）**：扫描完成后保存目录快照（路径 → size + mtime），下次扫描时先 diff 快照，只对变更的文件重新哈希/分类。**不需要 notify 依赖**，改动小。
- **阶段 B（notify 实时监听）**：引入 `notify` crate，对 watch 的目录实时监听 create/modify/delete，增量更新 DB。需要后台守护线程 + 事件去抖。

**建议先做阶段 A**（覆盖 80% 场景，成本低），阶段 B 作为后续增强。

**影响**：
- 阶段 A：新增 `core/snapshot.rs`（快照存取）+ scanner.rs（diff 逻辑）+ scan_tasks 表加快照 JSON 列
- 阶段 B：Cargo.toml 加 notify + 新增 `core/watcher.rs` + lib.rs 启动守护线程

**风险**：高。增量更新涉及 DB 一致性（删除的文件要标记、移动的文件要更新路径），需要仔细设计冲突处理

---

## P1：数据质量与可靠性

### P1-1：离线知识库预置数据（优先于 AI）

**问题**：offline_knowledge.db 是 0 字节空文件，离线匹配永远 miss，所有补全都依赖 LLM（慢 + 不稳定 + 可能编造）。

**改动**：
1. 预置常见安全/开发工具的 SQLite 知识库（7-Zip / IDA / Burp / hashcat / yakit / frp / Cobalt Strike 等 200+ 条）
2. 数据来源：CSV → SQLite 生成脚本（类似 软件分类.csv 的 include_str! 模式）
3. OfflineEnricher 打开预置库而非空文件
4. FallbackEnricher 顺序：**离线库（主）→ LLM（备）**，离线命中就不再调 LLM

**影响**：新增 `resources/offline_knowledge.db`（预置）或 seed 逻辑 + enrich.rs 配置路径
**风险**：低。纯数据补充

### P1-2：AI 补全置信度管控（防编造）

**问题**：LLM 对不认识的工具可能编造官网/GitHub 地址。

**改动**：
1. AI 返回的结果 confidence < 0.6 → 标记 `needs_review=true`，不直接写入 homepage_url（写入 notes 字段待确认）
2. AI 调用按归一化名称做本地缓存（同一软件不同版本不重复调模型）
3. 富集前先查离线库缓存，命中则跳过 LLM

**影响**：enricher.rs（parse_enrich_response 置信度门槛）+ enrich.rs（缓存逻辑）
**风险**：低

### P1-3：quarantine 隔离目录（替代系统回收站）

**问题**：系统回收站不同盘符行为不一致、清空不可控。

**改动**：
1. Executor 删除改为移入独立 quarantine 目录（如 `<data_dir>/quarantine/<session_id>/`）
2. 保留 quarantine 元数据：原始路径 → quarantine 路径 → 计划清空时间（默认 7 天）
3. 定期清理 quarantine 过期项
4. 操作日志明确记录移动链路，支持精确回滚

**影响**：executor.rs（recycle_file → quarantine）+ 可能新增 quarantine 清理定时任务
**风险**：中。跨盘移动性能（quarantine 目录应优先放被删文件同盘）

---

## P2：性能与体验优化

### P2-1：aho-corasick 关键词匹配（分类引擎加速）

**问题**：功能分类用线性匹配（规则量小暂够，但用户自定义规则增多后会慢）。

**改动**：classifier.rs 的 `classify_functional` 改用 aho-corasick 自动机（规则热加载时重建一次，几十万文件名一次线性扫描）。Cargo.toml 加 aho-corasick。

**风险**：低。纯性能优化，行为不变

### P2-2：哈希惰性计算深化

**问题**：当前扫描阶段仍对所有内容文件算 partial hash，理想情况是"用到才算"。

**改动**：扫描阶段只记录 size + mtime，hash 在 dedup 检测时按需计算（size 分组后组内才算）。

**风险**：中。需要重构 dedup 检测流程

---

## 建议执行顺序

```
P0-1（默认排除）→ P0-2（OneDrive 检测）→ P0-3（哈希分层）→ P1-1（离线知识库）
                                                            ↓
                                              P0-4 阶段 A（快照 diff）→ P1-2/P1-3 → P0-4 阶段 B（notify）
```

**第一批（立即可做，1-2 天）**：P0-1 + P0-2，直接解决"翻车"风险
**第二批（1 周）**：P0-3 + P1-1，性能 + 数据质量
**第三批（2 周）**：P0-4 阶段 A，增量扫描基础
**后续**：P1-2 / P1-3 / P2 / P0-4 阶段 B

---

## 开放问题（需用户决策）

1. **P0-3 blake3 兼容性**：换 hash 算法会导致已有 DB 的 hash 失效。是全量重扫还是做 hash 版本标记？
2. **P0-4 增量策略**：阶段 A（快照 diff）还是直接上阶段 B（notify 实时）？取决于你的使用场景（定期整理 vs 实时监控）。
3. **P1-3 quarantine 位置**：放系统数据目录（统一管理）还是被删文件同盘（移动快）？
4. **离线知识库数据来源**：要我预置一份常见工具库（基于安全/开发场景），还是你提供具体清单？
