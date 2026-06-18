<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import { listen, type UnlistenFn } from "@/lib/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "@/stores/settings";
import { useFilesStore } from "@/stores/files";
import { isHeadless } from "@/headless-patch";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { TooltipProvider } from "radix-vue";
import {
  Folder, Search, Scan, Tag, BookOpen, Sparkles,
  Settings, ScrollText, ChevronLeft, ChevronRight,
  FolderOpen, Copy, Layers, Menu,
  Minus, Square, X,
} from "lucide-vue-next";

const router = useRouter();
const route = useRoute();
const settingsStore = useSettingsStore();
const filesStore = useFilesStore();
const headless = isHeadless();

const ruleItems: { key: keyof typeof settingsStore.config.rules; label: string }[] = [
  { key: "auto_categorize", label: "安装包归类" },
  { key: "auto_duplicate", label: "自动去重" },
  { key: "keep_newest_version", label: "版本保留最新" },
  { key: "move_to_recycle_bin", label: "移至回收站" },
  { key: "delete_empty_dirs", label: "删除空目录" },
];

let appWindow: any = null;
try {
  appWindow = getCurrentWindow();
} catch {
  // headless 模式下无窗口
}

const rightPanelOpen = ref(true);
const sidebarCollapsed = ref(false);

async function minimizeWindow() {
  await appWindow?.minimize();
}
async function maximizeWindow() {
  await appWindow?.toggleMaximize();
}
async function closeWindow() {
  await appWindow?.close();
}

const unlisteners = ref<UnlistenFn[]>([]);

const mainNavItems = [
  { path: "/files", label: "全部文件", icon: Folder, query: {} },
  { path: "/files", label: "重复文件", icon: Copy, query: { dup: "1" } },
  { path: "/files", label: "多版本", icon: Layers, query: { mv: "1" } },
];

const bottomNavItems = [
  { path: "/scan", label: "扫描", icon: Scan },
  { path: "/catalog", label: "软件目录", icon: BookOpen },
  { path: "/enrich", label: "AI丰富", icon: Sparkles },
  { path: "/categories", label: "分类管理", icon: FolderOpen },
  { path: "/tags", label: "标签管理", icon: Tag },
  { path: "/logs", label: "操作日志", icon: ScrollText },
  { path: "/settings", label: "设置", icon: Settings },
];

function isActive(path: string, query?: Record<string, string>) {
  if (route.path !== path) return false;
  if (!query) return true;
  return Object.entries(query).every(([k, v]) => route.query[k] === v);
}

function navigateTo(path: string, query?: Record<string, string>) {
  if (query && Object.keys(query).length > 0) {
    router.push({ path, query });
  } else {
    router.push(path);
  }
}

function formatTime(seconds: number) {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
}

onMounted(async () => {
  await settingsStore.fetchSettings();
  await settingsStore.fetchRules();
  await filesStore.setupListeners();

  const un1 = await listen("scan_complete", () => {
    filesStore.fetchStats();
  });
  const un2 = await listen("clean_complete", () => {
    filesStore.fetchStats();
    filesStore.fetchFiles();
  });
  const un3 = await listen("enrich_complete", () => {
    // refresh handled by enrich view
  });
  unlisteners.value = [un1, un2, un3];
});

onUnmounted(() => {
  filesStore.cleanupListeners();
  unlisteners.value.forEach((fn) => fn());
});
</script>

<template>
  <TooltipProvider>
    <div class="flex flex-col h-screen bg-background">
      <!-- Custom Title Bar (GUI mode only) -->
      <div
        v-if="!headless"
        class="flex items-center h-8 bg-card border-b shrink-0 select-none"
        data-tauri-drag-region
      >
        <div class="flex items-center gap-2 px-3" data-tauri-drag-region>
          <Folder class="h-4 w-4 text-primary" />
          <span class="text-xs font-semibold">FileSweep</span>
        </div>
        <div class="flex-1" data-tauri-drag-region />
        <div class="flex items-center h-full">
          <button
            class="flex items-center justify-center w-11 h-full hover:bg-accent transition-colors"
            @click="minimizeWindow"
          >
            <Minus class="h-3.5 w-3.5" />
          </button>
          <button
            class="flex items-center justify-center w-11 h-full hover:bg-accent transition-colors"
            @click="maximizeWindow"
          >
            <Square class="h-3 w-3" />
          </button>
          <button
            class="flex items-center justify-center w-11 h-full hover:bg-red-500 hover:text-white transition-colors"
            @click="closeWindow"
          >
            <X class="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      <!-- Main Content -->
      <div class="flex flex-1 overflow-hidden">
      <!-- Left Sidebar -->
      <aside
        :class="[
          'flex flex-col border-r bg-card transition-all duration-200',
          sidebarCollapsed ? 'w-0 overflow-hidden' : 'w-[200px]',
        ]"
      >
        <ScrollArea class="flex-1">
          <!-- Main Nav -->
          <div class="p-3">
            <p class="text-xs text-muted-foreground mb-2 px-1">文件</p>
            <div class="space-y-0.5">
              <button
                v-for="item in mainNavItems"
                :key="item.label"
                :class="[
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                  isActive(item.path, item.query as Record<string, string> | undefined)
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent text-foreground',
                ]"
                @click="navigateTo(item.path, item.query as Record<string, string> | undefined)"
              >
                <component :is="item.icon" class="h-4 w-4" />
                <span>{{ item.label }}</span>
                <Badge v-if="item.label === '重复文件' && filesStore.stats.duplicates > 0" variant="secondary" class="ml-auto text-[10px] px-1">
                  {{ filesStore.stats.duplicates }}
                </Badge>
              </button>
            </div>
          </div>

          <Separator class="my-1" />

          <!-- Category Nav from rules -->
          <div class="p-3">
            <p class="text-xs text-muted-foreground mb-2 px-1">分类</p>
            <div class="space-y-0.5">
              <button
                v-for="rule in settingsStore.rules"
                :key="rule.name"
                :class="[
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                  isActive('/files', { cat: rule.name })
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent text-foreground',
                ]"
                @click="navigateTo('/files', { cat: rule.name })"
              >
                <FolderOpen class="h-4 w-4" />
                <span>{{ rule.name }}</span>
              </button>
            </div>
          </div>

          <Separator class="my-1" />

          <!-- Bottom Nav -->
          <div class="p-3">
            <p class="text-xs text-muted-foreground mb-2 px-1">工具</p>
            <div class="space-y-0.5">
              <button
                v-for="item in bottomNavItems"
                :key="item.path"
                :class="[
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                  isActive(item.path)
                    ? 'bg-primary text-primary-foreground'
                    : 'hover:bg-accent text-foreground',
                ]"
                @click="navigateTo(item.path)"
              >
                <component :is="item.icon" class="h-4 w-4" />
                <span>{{ item.label }}</span>
              </button>
            </div>
          </div>
        </ScrollArea>
      </aside>

      <!-- Sidebar Toggle -->
      <button
        class="flex items-center justify-center w-5 border-r bg-card hover:bg-accent transition-colors"
        @click="sidebarCollapsed = !sidebarCollapsed"
      >
        <ChevronLeft v-if="!sidebarCollapsed" class="h-3 w-3" />
        <ChevronRight v-else class="h-3 w-3" />
      </button>

      <!-- Center Content -->
      <main class="flex-1 flex flex-col overflow-hidden">
        <div class="flex-1 overflow-auto">
          <router-view />
        </div>
      </main>

      <!-- Right Panel -->
      <aside
        v-if="rightPanelOpen"
        class="w-[210px] border-l bg-card flex flex-col"
      >
        <div class="flex items-center justify-between px-4 h-12 border-b">
          <span class="text-sm font-medium">自动化规则</span>
          <button class="text-muted-foreground hover:text-foreground" @click="rightPanelOpen = false">
            <ChevronRight class="h-4 w-4" />
          </button>
        </div>
        <ScrollArea class="flex-1 p-3">
          <div class="space-y-3">
              <div
                v-for="item in ruleItems"
                :key="item.key"
                class="flex items-center justify-between gap-2"
              >
                <span class="text-sm text-foreground">{{ item.label }}</span>
                <Switch
                  :model-value="settingsStore.config.rules[item.key] as boolean"
                  @update:model-value="() => settingsStore.toggleRule(item.key)"
              />
            </div>
          </div>
        </ScrollArea>
      </aside>

      <!-- Right Panel Toggle when closed -->
      <button
        v-if="!rightPanelOpen"
        class="flex items-center justify-center w-5 border-l bg-card hover:bg-accent transition-colors"
        @click="rightPanelOpen = true"
      >
        <ChevronLeft class="h-3 w-3" />
      </button>
    </div>
    </div>
  </TooltipProvider>
</template>
