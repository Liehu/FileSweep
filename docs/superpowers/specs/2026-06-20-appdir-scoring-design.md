# app dir 综合判定模型设计

**日期**：2026-06-20
**状态**：设计阶段
**背景**：替代当前硬编码阈值（≥5 exe / ≥10 压缩包 / ≥2 子目录）的多条件判定

---

## 核心思路

给每个候选目录算"app dir 置信度分数"，基于三维度加权信号综合判断，替代脆弱的数量阈值。

## 三维度信号

### 维度1：文件类型占比画像（DirProfile）

扩展 SubtreeStats，统计子树的 5 类文件：

| 类型 | 后缀 | 信号意义 |
|---|---|---|
| exec | exe/jar/app/bat/cmd | 可执行；占比高+数量多→集合；占比低+有 dll→单软件 |
| archive | zip/rar/7z/gz/tar/bz2/xz/iso | 压缩包；占比高→归档集合 |
| doc | doc/pdf/ppt/xls/md/txt/rtf | 文档；占比中→CTF/资料目录 |
| script | py/sh/ps1/rb/pl | 脚本；占比极高+无 exec→Python 项目 |
| data | dll/so/dat/db/json/xml/yaml/ini/cfg/conf | 依赖/数据；占比高→单软件运行时 |

统计方式：阶段2 mark_subtree_stats 自底向上累加（与现有 has_exec 同理）。

### 维度2：目录层级深度

- 深度 ≤2：更可能是软件根（+1 分）
- 深度 ≥3：更可能是依赖子目录（向上合并倾向）

### 维度3：子目录结构模式

- 配套子目录（is_data_dir_name 命中：data/lib/bin/locale...）→ 单软件信号（+1 分）
- 独立软件子目录（非 data_dir_name 且是 candidate）≥2 → 集合信号（-1 分）
- 版本号子目录（1.8.3/v2.0）→ 依赖信号（向上合并）

## 评分规则

```
app_dir_score:
  +2  if data_ratio > 0.4          // dll/依赖主导（ztasker 的 40+ dll）
  +1  if 有配套子目录（data/lib/bin） // 单软件结构
  +1  if 层级 ≤2                    // 浅层更可能是软件根
  +1  if exec_ratio 0.05~0.3 且 data_ratio > 0.2  // exe+dll 混合（典型软件）
  -2  if archive_ratio > 0.5       // 压缩包主导（Compressed）
  -2  if exec_ratio > 0.05 且独立 exe ≥5  // 散装 exe 集合（Programs）
  -1  if 独立软件子目录 ≥2           // 多软件（shiro）
  -1  if script_ratio > 0.8 且无 exec  // Python 项目（特殊：app dir 但 reason=python）

判定：
  python_project（script_ratio>0.8, 无 exec）→ app dir, reason=python-project
  score ≥ 2 → app dir
  score < 2 → 集合目录（跳过，内部展开）
```

## 验证矩阵

| 目录 | data_ratio | exec_ratio | archive_ratio | 子目录 | 层级 | score | 判定 |
|---|---|---|---|---|---|---|---|
| ztasker | 高(40+ dll) | 低(4) | 0 | Data/User(配套) | 1 | +2+1+1=4 | app dir ✓ |
| chrome-win | 低 | 中(6) | 0 | 无 | 1 | +1=1... 需调 | app dir（小目录特殊） |
| shiro_attack | 低 | jar主导 | 0 | lib(依赖) | 2 | +1+1=2 | app dir ✓ |
| Programs | 低 | 高(30+) | 0 | configs/logs(配套) | 1 | -2+1=-1 | 集合 ✓ |
| Compressed | 0 | 0 | 极高(50+) | 2软件 | 1 | -2+...=-1 | 集合 ✓ |
| sqlmap | 0 | 0 | 0 | lib | 2 | python特例 | app dir ✓ |
| 红明谷 | 低 | 低(1) | 低 | 无 | 1 | +1+? | app dir（需调） |

> chrome-win 和红明谷这种"小目录"可能 score 不够，需设特殊规则：总文件数 ≤30 且有 exec → 直接判 app dir。

## 实现

1. 扩展 SubtreeStats：加 archive_count/doc_count/script_count/data_count
2. 扩展 stats_for_file：返回 5 类标记
3. 新增 `compute_app_dir_score(dir, tree, stats) -> i32`
4. find_app_roots 用 score 替代 is_software_collection_dir
5. 小目录特殊规则：total_files ≤30 且 has_exec → app dir（避免 chrome-win 等漏判）
