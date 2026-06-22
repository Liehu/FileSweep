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

  const hasSelection = computed(() => selectedIds.value.size > 0);

  async function fetchFiles(category?: string, status?: string) {
    loading.value = true;
    error.value = null;
    try {
      const params: Record<string, any> = {
        page: page.value,
        page_size: pageSize.value,
      };
      if (category) params.category = category;
      if (status) params.status = status;
      if (searchQuery.value) params.search = searchQuery.value;
      const res = await pluginInvoke<any>("filesweep", "scan:files", params);
      console.log("[fetchFiles] params:", params, "response:", res);
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
      console.error("[fetchFiles] ERROR:", e);
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
      // invoke 立即返回，扫描在后台进行，scan_complete 事件触发刷新
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
    const unlisten4 = await listen("clean_complete", () => {
      fetchStats();
      fetchFiles();
    });
    const unlisten5 = await listen<string>("clean_error", (e) => {
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
    selectedIds, filterCategory, searchQuery, suggestions, lastScanDir,
    scanState, scanProgress, hasSelection,
    fetchFiles, fetchStats, fetchSuggestions, startScan, cancelScan,
    setAction, setMoveTarget, batchSetAction, executeCleanup,
    toggleSelect, toggleSelectAll, clearSelection, setFilterCategory,
    setupListeners, cleanupListeners,
  };
});
