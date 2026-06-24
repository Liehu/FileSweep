import { defineStore } from "pinia";
import { ref } from "vue";
import { pluginInvoke } from "@/lib/pluginInvoke";

/** 候选迁移目录 */
export interface CandidateDir {
  path: string;
  name: string;
  watch_root: string;
  size_bytes: number;
  file_count: number;
  is_junction: boolean;
  description: string;
  software_name: string;
}

export interface TargetMap {
  source_root: string;
  target_root: string;
}

export interface ProtectedEntry {
  path: string;
  source: string;
}

export interface MigrateJob {
  id: number;
  source_path: string;
  target_path: string;
  status: string;
  checkpoint: string[];
  file_count: number;
  copied_count: number;
  total_bytes: number;
  started_at: number | null;
  finished_at: number | null;
  error: string;
}

export interface LockReport {
  blocking_processes: { pid: number; name: string; exe_path: string }[];
  shell_loaded_dlls: string[];
  need_explorer_restart: boolean;
  safe: boolean;
}

export interface MigratePlan {
  source_path: string;
  target_path: string;
  size_bytes: number;
  file_count: number;
  target_free_bytes: number;
  space_ok: boolean;
  locks: LockReport;
}

export interface MonitorEvent {
  watch_root: string;
  dir_name: string;
  full_path: string;
  state: string;
  first_seen_at: number;
  last_seen_at: number;
}

export interface EnvBackupEntry {
  id: number;
  scope: string;
  key: string;
  value: string;
  backed_up_at: number;
}

export interface UninstallEntry {
  name: string;
  version: string;
  publisher: string;
  install_location: string;
  uninstall_string: string;
}

const PLUGIN = "appmover";

export const useAppMoverStore = defineStore("appmover", () => {
  const candidates = ref<CandidateDir[]>([]);
  const targetMap = ref<TargetMap[]>([]);
  const protectedSet = ref<ProtectedEntry[]>([]);
  const jobs = ref<MigrateJob[]>([]);
  const monitorEvents = ref<MonitorEvent[]>([]);
  const monitorRunning = ref(false);
  const envBackups = ref<EnvBackupEntry[]>([]);
  const installed = ref<UninstallEntry[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function call<T>(action: string, args?: Record<string, any>): Promise<T> {
    return pluginInvoke<T>(PLUGIN, action, args);
  }

  // ── 识别 ──
  async function scanCandidates(roots?: string[]) {
    loading.value = true;
    error.value = null;
    try {
      candidates.value = await call<CandidateDir[]>("am:scan_candidates", roots ? { roots } : {});
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function describe(dirName: string) {
    return call<{ dir_name: string; software_name: string; description: string; source: string }>(
      "am:describe",
      { dir_name: dirName },
    );
  }

  async function updateDescribe(dirName: string, softwareName: string, description: string) {
    await call("am:describe_update", { dir_name: dirName, software_name: softwareName, description });
  }

  // ── 基线 / 保护集 ──
  async function importBaselineFile(filePath: string) {
    return call<{ imported: number }>("am:import_baseline", { file_path: filePath });
  }

  async function setFirstScanAsBaseline() {
    return call<{ imported: number }>("am:set_first_scan_as_baseline");
  }

  async function fetchProtected() {
    protectedSet.value = await call<ProtectedEntry[]>("am:get_protected");
  }

  async function addProtected(name: string) {
    await call("am:add_protected", { name });
    await fetchProtected();
  }

  async function removeProtected(name: string) {
    await call("am:remove_protected", { name });
    await fetchProtected();
  }

  // ── 目标根映射 ──
  async function fetchTargetMap() {
    targetMap.value = await call<TargetMap[]>("am:get_target_map");
  }

  async function setTargetMap(sourceRoot: string, targetRoot: string) {
    await call("am:set_target_map", { source_root: sourceRoot, target_root: targetRoot });
    await fetchTargetMap();
  }

  async function removeTargetMap(sourceRoot: string) {
    await call("am:remove_target_map", { source_root: sourceRoot });
    await fetchTargetMap();
  }

  // ── 迁移 ──
  async function planMigration(sourcePath: string) {
    return call<MigratePlan>("am:plan_migration", { source_path: sourcePath });
  }

  async function scanLocks(dir: string) {
    return call<LockReport>("am:scan_locks", { dir });
  }

  async function killLocks(dir: string, force = false) {
    return call<{
      killed: any[];
      need_restart_explorer: boolean;
      explorer_restarted: boolean;
      manual: string[];
      safe: boolean;
      message: string;
    }>("am:kill_locks", { dir, force });
  }

  async function executeMigration(sourcePath: string, targetPath?: string) {
    return call<{ job_id: number; file_count: number; total_bytes: number }>(
      "am:execute_migration",
      targetPath ? { source_path: sourcePath, target_path: targetPath } : { source_path: sourcePath },
    );
  }

  async function retryMigration(jobId: number) {
    return call<{ file_count: number; total_bytes: number }>("am:retry_migration", { job_id: jobId });
  }

  async function fetchJobs(limit = 100) {
    jobs.value = await call<MigrateJob[]>("am:list_jobs", { limit });
  }

  // ── 监控 ──
  async function startMonitor(intervalSecs?: number) {
    const r = await call<{ running: boolean; interval_secs: number }>(
      "am:start_monitor",
      intervalSecs ? { interval_secs: intervalSecs } : {},
    );
    monitorRunning.value = r.running;
    return r;
  }

  async function stopMonitor() {
    await call("am:stop_monitor");
    monitorRunning.value = false;
  }

  async function fetchMonitorEvents() {
    const r = await call<{ events: MonitorEvent[]; running: boolean }>("am:get_monitor_events");
    monitorEvents.value = r.events;
    monitorRunning.value = r.running;
  }

  async function dismissEvent(watchRoot: string, dirName: string) {
    await call("am:dismiss_event", { watch_root: watchRoot, dir_name: dirName });
    await fetchMonitorEvents();
  }

  // ── 环境变量 / 卸载表 ──
  async function backupEnv(scope: string) {
    return call<{ backed_up: number }>("am:backup_env", { scope });
  }

  async function restoreEnv(scope: string, backedUpAt: number) {
    return call<{ restored: number }>("am:restore_env", { scope, backed_up_at: backedUpAt });
  }

  async function fetchEnvBackups(scope?: string) {
    envBackups.value = await call<EnvBackupEntry[]>(
      "am:list_env_backups",
      scope ? { scope } : {},
    );
  }

  async function fetchInstalled() {
    installed.value = await call<UninstallEntry[]>("am:list_installed");
  }

  // ── 托盘 / 自启 ──
  async function refreshBadge() {
    return call<{ count: number }>("am:refresh_badge");
  }

  async function getBadge() {
    return call<{ count: number }>("am:get_badge");
  }

  async function getAutostart() {
    return call<{ enabled: boolean }>("am:get_autostart");
  }

  async function setAutostart(enabled: boolean) {
    return call<{ enabled: boolean }>("am:set_autostart", { enabled });
  }

  return {
    candidates,
    targetMap,
    protectedSet,
    jobs,
    monitorEvents,
    monitorRunning,
    envBackups,
    installed,
    loading,
    error,
    scanCandidates,
    describe,
    updateDescribe,
    importBaselineFile,
    setFirstScanAsBaseline,
    fetchProtected,
    addProtected,
    removeProtected,
    fetchTargetMap,
    setTargetMap,
    removeTargetMap,
    planMigration,
    scanLocks,
    killLocks,
    executeMigration,
    retryMigration,
    fetchJobs,
    startMonitor,
    stopMonitor,
    fetchMonitorEvents,
    dismissEvent,
    backupEnv,
    restoreEnv,
    fetchEnvBackups,
    fetchInstalled,
    refreshBadge,
    getBadge,
    getAutostart,
    setAutostart,
  };
});
