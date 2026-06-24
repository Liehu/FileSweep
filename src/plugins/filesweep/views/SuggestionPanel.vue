<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { ChevronDown, ChevronRight, Trash2, Link2, CheckCircle2, AlertCircle, Package, Copy } from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";

const store = useFilesStore();
const expandedGroups = ref<Record<string, boolean>>({});

onMounted(() => {
  store.fetchSuggestionsV2();
});

function toggleGroup(key: string) {
  expandedGroups.value[key] = !expandedGroups.value[key];
}

function isExpanded(key: string): boolean {
  return expandedGroups.value[key] ?? true;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
  return `${(bytes / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

interface SuggestionItem {
  file_id: string;
  file_name: string;
  file_path: string;
  file_size: number;
  category: string;
  suggestion: string;
  confidence: string;
  reason: string;
  homepage_url: string;
  auto_checked: boolean;
  keep_id?: string;
  keep_name?: string;
}

const groups = computed(() => {
  const s = store.suggestionSummary;
  if (!s) return [];
  return [
    { key: "high", label: "高置信建议", icon: CheckCircle2, items: (s.high_confidence || []) as SuggestionItem[], color: "text-green-500" },
    { key: "medium", label: "需确认", icon: AlertCircle, items: (s.medium_confidence || []) as SuggestionItem[], color: "text-yellow-500" },
    { key: "old", label: "旧版本", icon: Package, items: (s.old_versions || []) as SuggestionItem[], color: "text-blue-500" },
    { key: "dup", label: "重复文件", icon: Copy, items: (s.duplicates || []) as SuggestionItem[], color: "text-purple-500" },
  ].filter((g) => g.items.length > 0);
});

const totalSize = computed(() => store.suggestionSummary?.total_size || 0);
const totalItems = computed(() => store.suggestionSummary?.total_items || 0);
const keptCount = computed(() => store.suggestionSummary?.kept || 0);

function getSuggestionIcon(suggestion: string) {
  switch (suggestion) {
    case "downgrade": return Link2;
    case "delete_old": return Package;
    case "delete_dup": return Copy;
    default: return Trash2;
  }
}

function getSuggestionLabel(suggestion: string) {
  switch (suggestion) {
    case "downgrade": return "降级为链接";
    case "delete_old": return "删除旧版";
    case "delete_dup": return "删除副本";
    default: return suggestion;
  }
}

const checkedIds = ref<Set<string>>(new Set());

function toggleCheck(id: string) {
  if (checkedIds.value.has(id)) {
    checkedIds.value.delete(id);
  } else {
    checkedIds.value.add(id);
  }
}
</script>

<template>
  <div class="flex flex-col h-full">
    <!-- 摘要栏 -->
    <div class="p-4 border-b bg-card">
      <div class="flex items-center justify-between">
        <div>
          <h3 class="text-lg font-semibold">智能建议</h3>
          <p class="text-sm text-muted-foreground mt-1">
            {{ totalItems }} 个文件建议清理（预计释放 {{ formatSize(totalSize) }}）
            <span class="ml-2 text-xs">{{ keptCount }} 个文件保留</span>
          </p>
        </div>
        <Button variant="default" size="sm" :disabled="checkedIds.size === 0">
          <Trash2 class="h-4 w-4 mr-2" />
          执行清理（{{ checkedIds.size }}）
        </Button>
      </div>
    </div>

    <!-- 分组列表 -->
    <ScrollArea class="flex-1">
      <div class="p-4 space-y-2">
        <div v-if="groups.length === 0 && store.suggestionSummary" class="text-center py-12 text-muted-foreground">
          <CheckCircle2 class="h-12 w-12 mx-auto mb-3 opacity-30" />
          <p>没有需要清理的文件</p>
        </div>

        <div v-for="group in groups" :key="group.key" class="border rounded-lg overflow-hidden">
          <!-- 组标题 -->
          <button class="flex items-center w-full p-3 hover:bg-accent transition-colors" @click="toggleGroup(group.key)">
            <component :is="group.icon" :class="['h-4 w-4 mr-2', group.color]" />
            <span class="font-medium text-sm flex-1 text-left">{{ group.label }}</span>
            <Badge variant="secondary" class="mr-2">
              {{ group.items.length }} 个（{{ formatSize(group.items.reduce((s, i) => s + i.file_size, 0)) }}）
            </Badge>
            <ChevronDown v-if="isExpanded(group.key)" class="h-4 w-4" />
            <ChevronRight v-else class="h-4 w-4" />
          </button>
          <!-- 组内容 -->
          <div v-if="isExpanded(group.key)" class="divide-y">
            <div
              v-for="item in group.items"
              :key="item.file_id"
              class="flex items-center gap-3 p-2 px-4 hover:bg-accent/50"
            >
              <input
                type="checkbox"
                :checked="checkedIds.has(item.file_id) || item.auto_checked"
                @change="toggleCheck(item.file_id)"
                class="rounded"
              />
              <component :is="getSuggestionIcon(item.suggestion)" class="h-4 w-4 text-muted-foreground shrink-0" />
              <div class="flex-1 min-w-0">
                <div class="text-sm truncate">{{ item.file_name }}</div>
                <div class="text-xs text-muted-foreground truncate">{{ item.reason }}</div>
              </div>
              <div class="text-xs text-muted-foreground shrink-0">{{ formatSize(item.file_size) }}</div>
              <Badge variant="outline" class="shrink-0 text-[10px]">
                {{ getSuggestionLabel(item.suggestion) }}
              </Badge>
              <a
                v-if="item.homepage_url"
                :href="item.homepage_url"
                target="_blank"
                class="text-xs text-primary hover:underline shrink-0"
              >
                官网
              </a>
            </div>
          </div>
        </div>
      </div>
    </ScrollArea>
  </div>
</template>
