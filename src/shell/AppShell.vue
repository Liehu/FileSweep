<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
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
import { ChevronLeft, ChevronRight, Folder, Minus, Square, X } from "lucide-vue-next";

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

// 动态分类导航组（从 settingsStore.rules 生成）
const categoryNav = computed(() => ({
  title: "分类",
  items: (settingsStore.rules ?? []).map((rule) => ({
    label: rule.name,
    route: "/files",
    query: { cat: rule.name },
  })),
}));

// 侧栏 badge 数据（key = itemLabel）
const badges = computed<Record<string, number | string | undefined>>(() => ({
  "重复文件": filesStore.stats.duplicates || undefined,
}));

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
        <!-- Left Sidebar（从插件 manifest 动态渲染 + 动态分类组） -->
        <Sidebar
          :class="sidebarCollapsed ? 'w-0 overflow-hidden' : 'w-[200px]'"
    :category-nav="categoryNav"
    :badges="badges"
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

        <!-- Right Panel -->
        <aside v-if="rightPanelOpen" class="w-[210px] border-l bg-card flex flex-col">
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

      <!-- Command Palette -->
      <CommandPalette v-model:open="paletteOpen" />
    </div>
  </TooltipProvider>
</template>
