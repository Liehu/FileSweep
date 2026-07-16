<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import { listen, type UnlistenFn } from "@/lib/api";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useSettingsStore } from "@plugins/filesweep/stores/settings";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { isHeadless } from "@/headless-patch";
import Sidebar from "./Sidebar.vue";
import CommandPalette from "./CommandPalette.vue";
import { Switch } from "@/components/ui/switch";
import { ScrollArea } from "@/components/ui/scroll-area";
import { TooltipProvider } from "radix-vue";
import { register, unregister } from "@tauri-apps/plugin-global-shortcut";
import {
  ChevronLeft, ChevronRight, Folder, FolderOpen, Copy, Layers, Files,
  Minus, Square, X,
} from "lucide-vue-next";

const router = useRouter();
const route = useRoute();
const settingsStore = useSettingsStore();
const filesStore = useFilesStore();
const headless = isHeadless();

let appWindow: any = null;
try {
  appWindow = getCurrentWindow();
} catch {
  // headless 模式下无窗口
}

const paletteOpen = ref(false);
const rightPanelOpen = ref(true);
const sidebarCollapsed = ref(false);

const ruleItems: { key: keyof typeof settingsStore.config.rules; label: string }[] = [
  { key: "auto_categorize", label: "安装包归类" },
  { key: "auto_duplicate", label: "自动去重" },
  { key: "keep_newest_version", label: "版本保留最新" },
  { key: "move_to_recycle_bin", label: "移至回收站" },
  { key: "delete_empty_dirs", label: "删除空目录" },
];

// 右侧文件分类导航项（视图层）
const fileViewItems = computed(() => [
  { label: "全部文件", icon: Files, query: {} as Record<string, string>, badge: undefined as number | undefined },
  { label: "重复文件", icon: Copy, query: { dup: "1" }, badge: filesStore.stats.duplicates || undefined },
  { label: "多版本文件", icon: Layers, query: { mv: "1" }, badge: filesStore.stats.multiversion || undefined },
]);

function isFileViewActive(item: { query: Record<string, string> }) {
  if (route.path !== "/files") return false;
  const keys = Object.keys(item.query);
  if (keys.length === 0) {
    // 全部文件：无 dup/mv/cat/dtype
    return !route.query.dup && !route.query.mv && !route.query.cat && !route.query.dtype;
  }
  return keys.every((k) => route.query[k] === item.query[k]);
}

function navigateToFileView(item: { query: Record<string, string> }) {
  if (Object.keys(item.query).length === 0) {
    router.push("/files");
  } else {
    router.push({ path: "/files", query: item.query });
  }
}

// 分类清单：从 settingsStore.rules（分类规则名）+ 文件统计派生
const categoryList = computed(() => {
  const rules = settingsStore.rules ?? [];
  return rules.map((r: any) => ({
    name: r.name,
    count: 0, // 计数需额外统计，暂留 0
  }));
});

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

  // 注册全局快捷键 Alt+Space 唤起命令面板
  try {
    await register("Alt+Space", () => {
      paletteOpen.value = !paletteOpen.value;
    });
  } catch (e) {
    console.warn("global shortcut register failed:", e);
  }
});

onUnmounted(() => {
  filesStore.cleanupListeners();
  unlisteners.value.forEach((fn) => fn());
  unregister("Alt+Space").catch(() => {});
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
        <!-- Left Sidebar（功能菜单） -->
        <Sidebar
          :class="sidebarCollapsed ? 'w-0 overflow-hidden' : 'w-[200px]'"
        />

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

        <!-- Right Panel：文件分类菜单 -->
        <aside v-if="rightPanelOpen" class="w-[210px] border-l bg-card flex flex-col">
          <div class="flex items-center justify-between px-4 h-12 border-b">
            <span class="text-sm font-medium">文件分类</span>
            <button class="text-muted-foreground hover:text-foreground" @click="rightPanelOpen = false">
              <ChevronRight class="h-4 w-4" />
            </button>
          </div>
          <ScrollArea class="flex-1">
            <!-- 文件分类导航 -->
            <div class="p-3 space-y-0.5">
              <p class="text-xs text-muted-foreground mb-1 px-1">视图</p>
              <button
                v-for="item in fileViewItems"
                :key="item.label"
                :class="[
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                  isFileViewActive(item) ? 'bg-primary text-primary-foreground' : 'hover:bg-accent text-foreground',
                ]"
                @click="navigateToFileView(item)"
              >
                <component :is="item.icon" class="h-4 w-4" />
                <span class="flex-1 text-left">{{ item.label }}</span>
                <span
                  v-if="item.badge != null"
                  class="text-[10px] px-1 rounded bg-muted text-muted-foreground"
                >
                  {{ item.badge }}
                </span>
              </button>
            </div>

            <div class="border-t my-1" />

            <!-- 分类清单（按文件 category 动态列出） -->
            <div class="p-3 space-y-0.5">
              <p class="text-xs text-muted-foreground mb-1 px-1">分类清单</p>
              <button
                v-for="cat in categoryList"
                :key="cat.name"
                :class="[
                  'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                  route.query.cat === cat.name ? 'bg-primary text-primary-foreground' : 'hover:bg-accent text-foreground',
                ]"
                @click="router.push({ path: '/files', query: { cat: cat.name } })"
              >
                <FolderOpen class="h-4 w-4" />
                <span class="flex-1 text-left truncate">{{ cat.name }}</span>
                <span class="text-[10px] text-muted-foreground">{{ cat.count }}</span>
              </button>
              <p v-if="categoryList.length === 0" class="text-xs text-muted-foreground px-2 py-1">
                暂无分类数据
              </p>
            </div>

            <div class="border-t my-1" />

            <!-- 自动化规则（折叠） -->
            <div class="p-3 space-y-2">
              <p class="text-xs text-muted-foreground mb-1 px-1">自动化规则</p>
              <div
                v-for="item in ruleItems"
                :key="item.key"
                class="flex items-center justify-between gap-2"
              >
                <span class="text-xs text-foreground">{{ item.label }}</span>
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

      <!-- Command Palette -->
      <CommandPalette v-model:open="paletteOpen" />
    </div>
  </TooltipProvider>
</template>
