import { defineStore } from "pinia";
import { ref, computed } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";
import { pluginInvoke } from "@/lib/pluginInvoke";

export interface FileItem {
  id: string;
  name: string;
  localPath: string;
  fileSize: number;
  extension: string;
  category: string;
  functionalCategory: string;
  isAppDir: boolean;
  version: string;
  fileHash: string;
  aiSkip: boolean;
  scannedAt: string;
  modTime: string;
  catalogId: string;
  appDirPath: string;
  appDirReason: string;
  status: string;
  action?: string;
  move_target?: string;
  taskId?: string;
}

export interface ScanTask {
  id: string;
  scanDir: string;
  startedAt: string;
  finishedAt: string;
  fileCount: number;
  status: string;
  recursive: boolean;
}

export interface FileStats {
  total: number;
  totalSize: number;
  duplicates: number;
  multiversion: number;
  uncategorized: number;
}

export interface ScanProgress {
  total: number;
  done: number;
  currentFile: string;
  stage: string;
  stageLabel?: string;
  indeterminate?: boolean;
  ratePerSec?: number;
  etaSec?: number;
}

export const useFilesStore = defineStore("files", () => {
  const files = ref<FileItem[]>([]);
  const stats = ref<FileStats>({ total: 0, totalSize: 0, duplicates: 0, multiversion: 0, uncategorized: 0 });
  const loading = ref(false);
  const error = ref<string | null>(null);

  const page = ref(1);
  const pageSize = ref(20);
  const total = ref(0);
  const totalPages = ref(0);

  const selectedIds = ref<Set<string>>(new Set());
  const filterCategory = ref<string>("");
  const searchQuery = ref("");
  const suggestions = ref<Record<string, string>>({});
  const lastScanDir = ref<string[]>([]);
  const scanState = ref<"idle" | "scanning" | "paused" | "done" | "error">("idle");
  const scanProgress = ref<ScanProgress | null>(null);

  // 扫描任务列表 + 当前筛选的 task_id（空 = 全部文件）
  const scanTasks = ref<ScanTask[]>([]);
  const filterTaskId = ref("");

  const hasSelection = computed(() => selectedIds.value.size > 0);

  async function fetchFiles(category?: string, status?: string, dirType?: string) {
    loading.value = true;
    error.value = null;
    try {
      const params: Record<string, any> = {
        page: page.value,
        page_size: pageSize.value,
      };
      if (category) params.category = category;
      if (status) params.status = status;
      if (dirType) params.dir_type = dirType;
      if (filterTaskId.value) params.task_id = filterTaskId.value;
      if (searchQuery.value) params.search = searchQuery.value;
      const res = await pluginInvoke<any>("filesweep", "scan:files", params);
      const rawFiles = res.files || res.items || [];
      // 预填充建议操作到 action 字段
      for (const f of rawFiles) {
        if (!f.action && suggestions.value[f.id]) {
          f.action = suggestions.value[f.id];
        }
      }
      files.value = rawFiles;
      total.value = res.total || 0;
      totalPages.value = Math.ceil(total.value / pageSize.value);
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function fetchStats() {
    try {
      const res = await pluginInvoke<FileStats>("filesweep", "scan:stats");
      stats.value = res;
    } catch (e) {
      console.error("Failed to fetch stats:", e);
    }
  }

  async function fetchScanTasks() {
    try {
      const res = await pluginInvoke<ScanTask[]>("filesweep", "scan:tasks:list", { limit: 50 });
      scanTasks.value = res || [];
    } catch (e) {
      console.error("Failed to fetch scan tasks:", e);
      scanTasks.value = [];
    }
  }

  async function deleteScanTask(id: string) {
    try {
      await pluginInvoke("filesweep", "scan:tasks:delete", { id });
      await fetchScanTasks();
    } catch (e) {
      error.value = String(e);
    }
  }

  function setFilterTask(id: string) {
    filterTaskId.value = id;
    page.value = 1;
  }

  async function fetchSuggestions() {
    try {
      const res = await pluginInvoke<Array<{ fileId?: string; file_id?: string; action: string }>>("filesweep", "scan:suggestions");
      const map: Record<string, string> = {};
      for (const s of res) {
        const id = s.fileId || s.file_id || "";
        if (id) {
          map[id] = s.action;
        }
      }
      suggestions.value = map;
      // 预填充到已加载的文件
      for (const f of files.value) {
        if (!f.action && map[f.id]) {
          f.action = map[f.id];
        }
      }
    } catch (e) {
      console.error("Failed to fetch suggestions:", e);
    }
  }

  // 智能建议引擎 V2（分组返回）
  const suggestionSummary = ref<any>(null);

  // ── Everything 全局搜索（es.exe，失败回退 DB）──
  interface SearchResultItem {
    name: string;
    path: string;
    size: number;
  }
  const everythingQuery = ref("");
  const everythingResults = ref<SearchResultItem[]>([]);
  const everythingSource = ref<"everything" | "database" | "">("");
  const everythingSearching = ref(false);
  const everythingError = ref<string | null>(null);

  async function searchEverything(query: string) {
    everythingQuery.value = query;
    if (!query.trim()) {
      everythingResults.value = [];
      everythingSource.value = "";
      return;
    }
    everythingSearching.value = true;
    everythingError.value = null;
    try {
      const res = await pluginInvoke<any>("filesweep", "search", { query, max_results: 200 });
      // Everything 成功：返回 SearchResult[]（{name,path,size}）
      // DB 回退：返回 {results, total, source:"database"}
      if (Array.isArray(res)) {
        everythingResults.value = res as SearchResultItem[];
        everythingSource.value = "everything";
      } else if (res && Array.isArray(res.results)) {
        everythingResults.value = res.results as SearchResultItem[];
        everythingSource.value = (res.source as "everything" | "database") || "database";
      } else {
        everythingResults.value = [];
        everythingSource.value = "";
      }
    } catch (e) {
      everythingError.value = String(e);
      everythingResults.value = [];
      everythingSource.value = "";
    } finally {
      everythingSearching.value = false;
    }
  }

  function clearEverythingSearch() {
    everythingQuery.value = "";
    everythingResults.value = [];
    everythingSource.value = "";
    everythingError.value = null;
  }

  async function fetchSuggestionsV2() {
    try {
      const res = await pluginInvoke<any>("filesweep", "scan:suggestions_v2");
      suggestionSummary.value = res;
    } catch (e) {
      console.error("Failed to fetch suggestions v2:", e);
    }
  }

  async function startScan(dirs: string[], options?: { recursive?: boolean; excludeDirs?: string[]; excludeNames?: string[]; excludeExts?: string[]; detectAppDirs?: boolean }) {
    scanState.value = "scanning";
    scanProgress.value = null;
    lastScanDir.value = dirs;
    error.value = "";
    try {
      // scan:start 现在是后台执行，立即返回；通过 scan_complete 事件感知完成
      await pluginInvoke("filesweep", "scan:start", {
        dirs,
        recursive: options?.recursive ?? true,
        exclude_dirs: options?.excludeDirs ?? [],
        exclude_names: options?.excludeNames ?? [],
        exclude_exts: options?.excludeExts ?? [],
        detect_app_dirs: options?.detectAppDirs ?? false,
      });
      // invoke 立即返回，扫描在后台进行
      // 轮询 scan:status（AtomicBool，不查 DB，不竞争 lock）
      const pollInterval = setInterval(async () => {
        try {
          const status = await pluginInvoke<{ scanning: boolean }>("filesweep", "scan:status");
          if (!status.scanning) {
            clearInterval(pollInterval);
            scanState.value = "done";
            scanProgress.value = null;
            await fetchFiles();
            fetchStats();
          }
        } catch {}
      }, 1000);
      setTimeout(() => clearInterval(pollInterval), 300000);
    } catch (e) {
      scanState.value = "error";
      error.value = String(e);
    }
  }

  async function cancelScan() {
    try {
      await pluginInvoke("filesweep", "scan:cancel");
    } catch (e) {
      error.value = String(e);
    }
  }

  async function setAction(fileId: string, action: string, moveTarget?: string) {
    try {
      await pluginInvoke("filesweep", "files:set_action", { file_id: fileId, action, move_target: moveTarget });
    } catch (e) {
      error.value = String(e);
    }
  }

  async function setMoveTarget(fileId: string, target: string) {
    try {
      await pluginInvoke("filesweep", "files:set_move_target", { file_id: fileId, target });
    } catch (e) {
      error.value = String(e);
    }
  }

  async function batchSetAction(action: string, moveTarget?: string) {
    try {
      await pluginInvoke("filesweep", "files:batch_set_action", { file_ids: Array.from(selectedIds.value), action, move_target: moveTarget });
      selectedIds.value.clear();
    } catch (e) {
      error.value = String(e);
    }
  }

  async function executeCleanup(confirm: boolean = false) {
    try {
      const fileActions: Record<string, { action: string; move_target?: string }> = {};
      for (const file of files.value) {
        if (file.action) {
          fileActions[file.id] = { action: file.action, move_target: file.move_target };
        }
      }
      await pluginInvoke("filesweep", "clean:start", { confirm, file_actions: fileActions });
    } catch (e) {
      error.value = String(e);
    }
  }

  // 清理执行状态：idle / running / done / error
  const cleanState = ref<"idle" | "running" | "done" | "error">("idle");
  const cleanResult = ref<{ moved: number; deleted: number; failed: number; dry_run: boolean } | null>(null);

  /**
   * 按建议面板勾选项执行清理。
   *
   * SuggestionItem.suggestion → executor action 映射：
   *   delete / delete_old / delete_dup / downgrade → "delete"
   *     （downgrade = 删除安装包，链接信息已在 catalog；delete = 临时文件直接删除）
   *   move → "move"（整目录迁移，需带 move_target）
   *   其他 → 跳过
   *
   * 后端 clean:start 期望 file_actions 为数组，每项含
   * {id, action, name, local_path, file_hash, file_size, extension, move_target}
   */
  async function executeSuggestionCleanup(items: Array<{
    file_id: string;
    file_name: string;
    file_path: string;
    file_size: number;
    suggestion: string;
    move_target?: string;
  }>, confirm: boolean = false) {
    cleanState.value = "running";
    cleanResult.value = null;
    error.value = null;
    try {
      const fileActions = items
        .map((it) => {
          // delete / downgrade / delete_old / delete_dup → 后端 delete
          // move → 后端 move（带 move_target）
          let action = "";
          let moveTarget = "";
          if (
            it.suggestion === "delete" ||
            it.suggestion === "downgrade" ||
            it.suggestion === "delete_old" ||
            it.suggestion === "delete_dup"
          ) {
            action = "delete";
          } else if (it.suggestion === "move") {
            action = "move";
            moveTarget = it.move_target || "";
          }
          if (!action) return null;
          return {
            id: it.file_id,
            action,
            name: it.file_name,
            local_path: it.file_path,
            file_size: it.file_size,
            extension: "",
            move_target: moveTarget,
          };
        })
        .filter((x): x is NonNullable<typeof x> => x !== null);

      if (fileActions.length === 0) {
        cleanState.value = "done";
        cleanResult.value = { moved: 0, deleted: 0, failed: 0, dry_run: !confirm };
        return;
      }

      await pluginInvoke("filesweep", "clean:start", { confirm, file_actions: fileActions });
      // 实际完成通过 clean_complete 事件感知（见 setupListeners），这里乐观置 done
      cleanState.value = "done";
    } catch (e) {
      cleanState.value = "error";
      error.value = String(e);
    }
  }

  /**
   * 一键清理：从 v2 建议引擎（suggestionSummary）收集所有 auto_checked 项，
   * 转 file_actions 执行。覆盖重复/多版本/临时文件/降级等全部建议类型。
   *
   * 这是 FileListView「执行清理」按钮的正确入口（替代旧的 executeCleanup，
   * 后者依赖不全的旧建议 API）。
   */
  async function executeCleanupFromSuggestions(confirm: boolean = true) {
    const s = suggestionSummary.value;
    if (!s) {
      error.value = "建议数据未加载，请先扫描";
      return;
    }
    // 合并所有分组，只取 auto_checked=true 的项
    const allItems = [
      ...(s.high_confidence || []),
      ...(s.medium_confidence || []),
      ...(s.old_versions || []),
      ...(s.duplicates || []),
    ].filter((it: any) => it.auto_checked);

    if (allItems.length === 0) {
      cleanState.value = "done";
      cleanResult.value = { moved: 0, deleted: 0, failed: 0, dry_run: !confirm };
      return;
    }

    await executeSuggestionCleanup(
      allItems.map((it: any) => ({
        file_id: it.file_id,
        file_name: it.file_name,
        file_path: it.file_path,
        file_size: it.file_size,
        suggestion: it.suggestion,
        move_target: it.move_target,
      })),
      confirm
    );
  }

  function toggleSelect(id: string) {
    if (selectedIds.value.has(id)) {
      selectedIds.value.delete(id);
    } else {
      selectedIds.value.add(id);
    }
    selectedIds.value = new Set(selectedIds.value);
  }

  function toggleSelectAll() {
    if (selectedIds.value.size === files.value.length) {
      selectedIds.value.clear();
    } else {
      selectedIds.value = new Set(files.value.map((f) => f.id));
    }
  }

  function clearSelection() {
    selectedIds.value.clear();
  }

  function setFilterCategory(cat: string) {
    filterCategory.value = cat;
    page.value = 1;
  }

  let _unlisteners: UnlistenFn[] = [];

  async function setupListeners() {
    const unlisten1 = await listen<ScanProgress>("scan_progress", (e) => {
      scanProgress.value = e.payload;
    });
    const unlisten2 = await listen("scan_complete", () => {
      scanState.value = "done";
      scanProgress.value = null;
      fetchStats();
      fetchFiles();
    });
    const unlisten3 = await listen<string>("scan_error", (e) => {
      scanState.value = "error";
      error.value = e.payload;
    });
    const unlisten4 = await listen<{ moved: number; deleted: number; failed: number; dry_run: boolean }>("clean_complete", (e) => {
      cleanResult.value = e.payload;
      cleanState.value = "done";
      fetchStats();
      fetchFiles();
      // 清理完成后刷新建议（已删除的文件不再出现在建议中）
      fetchSuggestionsV2();
    });
    const unlisten5 = await listen<string>("clean_error", (e) => {
      cleanState.value = "error";
      error.value = e.payload;
    });
    const unlisten6 = await listen("scan_cancelled", () => {
      scanState.value = "idle";
      scanProgress.value = null;
    });
    _unlisteners = [unlisten1, unlisten2, unlisten3, unlisten4, unlisten5, unlisten6];
  }

  function cleanupListeners() {
    _unlisteners.forEach((fn) => fn());
    _unlisteners = [];
  }

  return {
    files, stats, loading, error, page, pageSize, total, totalPages,
    selectedIds, filterCategory, searchQuery, suggestions, suggestionSummary, lastScanDir,
    scanState, scanProgress, hasSelection,
    scanTasks, filterTaskId,
    cleanState, cleanResult,
    everythingQuery, everythingResults, everythingSource, everythingSearching, everythingError,
    searchEverything, clearEverythingSearch,
    fetchFiles, fetchStats, fetchSuggestions, fetchSuggestionsV2, startScan, cancelScan,
    fetchScanTasks, deleteScanTask, setFilterTask,
    setAction, setMoveTarget, batchSetAction, executeCleanup, executeSuggestionCleanup, executeCleanupFromSuggestions,
    toggleSelect, toggleSelectAll, clearSelection, setFilterCategory,
    setupListeners, cleanupListeners,
  };
});
