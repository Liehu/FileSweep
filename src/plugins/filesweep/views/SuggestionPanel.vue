<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";
import { pluginInvoke } from "@/lib/pluginInvoke";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { useCatalogStore } from "@plugins/filesweep/stores/catalog";
import { useSettingsStore } from "@plugins/filesweep/stores/settings";
import {
  ChevronDown, ChevronRight, Trash2, Link2, CheckCircle2, AlertCircle,
  Package, Copy, CheckSquare, Square, FolderInput, Sparkles, Play, Pencil, X, Clock,
} from "lucide-vue-next";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import { Card, CardContent } from "@/components/ui/card";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import {
  Dialog, DialogContent, DialogHeader, DialogFooter, DialogTitle, DialogDescription,
} from "@/components/ui/dialog";

const store = useFilesStore();
const catalogStore = useCatalogStore();
const settingsStore = useSettingsStore();
const expandedGroups = ref<Record<string, boolean>>({});

// ── AI 丰富（整合自 EnrichView，修复：改用 pluginInvoke + 审核真实写回 catalog）──
const enrichSectionOpen = ref(true);
// 默认 provider：优先用已配置 key 的 LLM，避免默认 offline（离线库可能为空导致啥也匹配不到）
const enrichProvider = ref("offline");
const enriching = ref(false);
const enrichProgress = ref(0);
const enrichProgressText = ref("");
const enrichError = ref<string | null>(null);
const enrichUnlisteners = ref<UnlistenFn[]>([]);

/// 下拉可选项：只显示已配置凭据的 provider，避免选到无效项
const providerOptions = computed(() => {
  const ai = settingsStore.config.ai;
  const opts: { value: string; label: string }[] = [];
  if (ai.custom_base_url && ai.custom_api_key) {
    opts.push({ value: "custom", label: `自定义 (${ai.custom_name || ai.custom_base_url})` });
  }
  if (ai.openai_api_key) {
    opts.push({ value: "openai", label: "OpenAI" });
  }
  if (ai.claude_api_key) {
    opts.push({ value: "claude", label: "Claude" });
  }
  if (ai.ollama_url) {
    opts.push({ value: "ollama", label: `Ollama (${ai.ollama_model || "默认"})` });
  }
  opts.push({ value: "offline", label: "离线规则（仅内置库）" });
  return opts;
});

/// 根据已配置的凭据自动选最优 provider（避免默认 offline 匹配不到）
function pickDefaultProvider(): string {
  const ai = settingsStore.config.ai;
  if (ai.custom_base_url && ai.custom_api_key) return "custom";
  if (ai.openai_api_key) return "openai";
  if (ai.claude_api_key) return "claude";
  if (ai.ollama_url) return "ollama";
  return "offline";
}

// 待审核 = needsReview=true 的 catalog 条目；已丰富 = needsReview=false 且 aiProvider 非空
const reviewQueue = computed(() =>
  catalogStore.entries.filter((e) => e.needsReview && !e.aiSkip)
);
const enrichedCount = computed(() =>
  catalogStore.entries.filter((e) => !e.needsReview && e.aiProvider && e.aiProvider !== "manual").length
);

async function startEnrich() {
  enriching.value = true;
  enrichProgress.value = 0;
  enrichProgressText.value = "准备中...";
  enrichError.value = null;
  try {
    // ✅ 修复：用插件 action 而非废弃的 invoke("start_enrich")
    await pluginInvoke("filesweep", "enrich:start", { provider: enrichProvider.value });
  } catch (e) {
    enriching.value = false;
    enrichError.value = String(e);
    enrichProgressText.value = "";
  }
}

// 中断 AI 补全：已完成的批次已落库，重启后自动从断点续传（跳过已丰富文件）
async function stopEnrich() {
  try {
    await pluginInvoke("filesweep", "enrich:cancel");
    enrichProgressText.value = "正在中断（等待当前批次完成）...";
  } catch (e) {
    enrichError.value = String(e);
  }
}

// 审核操作：真实写回 catalog
async function acceptEntry(id: string) {
  await catalogStore.updateEntry(id, { needsReview: false });
}
async function rejectEntry(id: string) {
  // 拒绝 = 标记 aiSkip，不再展示且不再丰富
  await catalogStore.updateEntry(id, { aiSkip: true, needsReview: false });
}

// 编辑弹窗
const editOpen = ref(false);
const editItem = ref<{ id: string; name: string } | null>(null);
const editForm = ref({ description: "", functionalCategory: "", tags: "" });
function openEdit(e: { id: string; name: string; description: string; functionalCategory: string; tags: string[] }) {
  editItem.value = { id: e.id, name: e.name };
  editForm.value = {
    description: e.description,
    functionalCategory: e.functionalCategory,
    tags: e.tags.join(", "),
  };
  editOpen.value = true;
}
async function saveEdit() {
  if (!editItem.value) return;
  await catalogStore.updateEntry(editItem.value.id, {
    description: editForm.value.description,
    functionalCategory: editForm.value.functionalCategory,
    needsReview: false,
  });
  editOpen.value = false;
}

async function setupEnrichListeners() {
  const un1 = await listen("enrich_progress", (e: any) => {
    const p = e.payload;
    enrichProgress.value = p.total > 0 ? Math.round((p.done / p.total) * 100) : 0;
    enrichProgressText.value = `${p.done}/${p.total} - ${p.currentName || ""}`;
  });
  const un2 = await listen("enrich_complete", () => {
    enriching.value = false;
    enrichProgress.value = 100;
    enrichProgressText.value = "完成";
    catalogStore.fetchCatalog();
    // 丰富完成后刷新建议（AI 补全的 download_reliability 会影响建议判定）
    store.fetchSuggestionsV2();
  });
  const un2b = await listen("enrich_cancelled", (e: any) => {
    enriching.value = false;
    const p = e.payload || {};
    enrichProgressText.value = `已中断（已保存 ${p.saved ?? 0} 个，可重新开始继续）`;
    catalogStore.fetchCatalog();
    store.fetchSuggestionsV2();
  });
  const un3 = await listen<string>("enrich_error", (e) => {
    enriching.value = false;
    enrichError.value = e.payload;
    enrichProgressText.value = "";
  });
  enrichUnlisteners.value = [un1, un2, un2b, un3];
}

onMounted(async () => {
  store.fetchSuggestionsV2();
  store.setupListeners();
  catalogStore.fetchCatalog();
  await settingsStore.fetchSettings();
  // settings 加载完后，自动选已配置的 provider（避免默认 offline 匹配不到）
  enrichProvider.value = pickDefaultProvider();
  setupEnrichListeners();
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

/// 规范化 URL：缺协议头时补 https://，避免被浏览器当相对 URL 拼到本地服务地址
function normalizeUrl(url: string): string {
  const trimmed = url.trim();
  if (!trimmed) return trimmed;
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
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
  move_target?: string;
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
    case "delete": return Trash2;
    case "move": return FolderInput;
    default: return Trash2;
  }
}

function getSuggestionLabel(suggestion: string) {
  switch (suggestion) {
    case "downgrade": return "降级为链接";
    case "delete_old": return "删除旧版";
    case "delete_dup": return "删除副本";
    case "delete": return "删除";
    case "move": return "迁移";
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

// 首次加载建议后，默认勾选所有 auto_checked 项
watch(
  () => store.suggestionSummary,
  (s) => {
    if (s && checkedIds.value.size === 0) {
      const auto = [
        ...(s.high_confidence || []),
        ...(s.old_versions || []),
        ...(s.duplicates || []),
      ] as SuggestionItem[];
      for (const it of auto) {
        if (it.auto_checked) checkedIds.value.add(it.file_id);
      }
    }
  },
  { immediate: true }
);

// 全选 / 取消全选（所有可清理项）
const allItems = computed(() => groups.value.flatMap((g) => g.items));
const allChecked = computed(() => allItems.value.length > 0 && allItems.value.every((i) => checkedIds.value.has(i.file_id)));
function toggleAll() {
  if (allChecked.value) {
    checkedIds.value.clear();
  } else {
    for (const it of allItems.value) checkedIds.value.add(it.file_id);
  }
}

// 收集勾选项（含文件元信息，供后端执行）
const checkedItems = computed(() =>
  allItems.value.filter((i) => checkedIds.value.has(i.file_id))
);

// 勾选项是否含迁移（整目录移动不可走回收站，确认对话框需特别提示）
const hasMoveItems = computed(() =>
  checkedItems.value.some((i) => i.suggestion === "move")
);

const showConfirm = ref(false);

function onClickClean() {
  if (checkedItems.value.length === 0) return;
  showConfirm.value = true;
}

async function confirmClean() {
  showConfirm.value = false;
  await store.executeSuggestionCleanup(
    checkedItems.value.map((i) => ({
      file_id: i.file_id,
      file_name: i.file_name,
      file_path: i.file_path,
      file_size: i.file_size,
      suggestion: i.suggestion,
      move_target: i.move_target,
    })),
    true // confirm=true，实际执行（入回收站）
  );
}

const cleanState = computed(() => store.cleanState);
const cleanResult = computed(() => store.cleanResult);
const cleanError = computed(() => store.error);

// 3 秒后自动隐藏结果横幅
watch(cleanState, (st) => {
  if (st === "done" || st === "error") {
    setTimeout(() => {
      if (store.cleanState === st) store.cleanState = "idle";
    }, 4000);
  }
});

onUnmounted(() => {
  enrichUnlisteners.value.forEach((fn) => fn());
});
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
        <div class="flex items-center gap-2">
          <Button variant="ghost" size="sm" @click="toggleAll" :disabled="allItems.length === 0">
            <component :is="allChecked ? CheckSquare : Square" class="h-4 w-4 mr-1" />
            {{ allChecked ? "取消全选" : "全选" }}
          </Button>
          <Button variant="default" size="sm" :disabled="checkedIds.size === 0 || cleanState === 'running'" @click="onClickClean">
            <Trash2 class="h-4 w-4 mr-2" />
            {{ cleanState === "running" ? "执行中..." : `执行清理（${checkedIds.size}）` }}
          </Button>
        </div>
      </div>
      <!-- 结果横幅 -->
      <div
        v-if="cleanState === 'done' && cleanResult"
        class="mt-2 flex items-center gap-2 text-sm rounded-md p-2 bg-green-500/10 text-green-600 dark:text-green-400"
      >
        <CheckCircle2 class="h-4 w-4" />
        <span>
          清理完成{{ cleanResult.dry_run ? "（预演）" : "" }}：删除 {{ cleanResult.deleted }} 项，移动 {{ cleanResult.moved }} 项
          <span v-if="cleanResult.failed > 0" class="text-red-500">，失败 {{ cleanResult.failed }} 项</span>
        </span>
      </div>
      <div
        v-else-if="cleanState === 'error'"
        class="mt-2 flex items-center gap-2 text-sm rounded-md p-2 bg-red-500/10 text-red-600 dark:text-red-400"
      >
        <AlertCircle class="h-4 w-4" />
        <span>清理失败：{{ cleanError }}</span>
      </div>
    </div>

    <!-- ════ AI 丰富区（可折叠）════ -->
    <Card class="border-dashed">
      <CardContent class="p-3 space-y-3">
        <button
          class="flex items-center w-full"
          @click="enrichSectionOpen = !enrichSectionOpen"
        >
          <Sparkles class="h-4 w-4 mr-2 text-primary" />
          <span class="font-medium text-sm flex-1 text-left">AI 元数据丰富</span>
          <Badge v-if="enrichedCount > 0" variant="secondary" class="text-[10px] mr-2">
            已丰富 {{ enrichedCount }}
          </Badge>
          <Badge v-if="reviewQueue.length > 0" variant="outline" class="text-[10px] mr-2 text-yellow-600">
            待审核 {{ reviewQueue.length }}
          </Badge>
          <ChevronDown v-if="enrichSectionOpen" class="h-4 w-4" />
          <ChevronRight v-else class="h-4 w-4" />
        </button>

        <div v-if="enrichSectionOpen" class="space-y-3">
          <!-- Provider 选择 + 开始按钮 -->
          <div class="flex items-end gap-2 flex-wrap">
            <div class="space-y-1">
              <Label class="text-xs">AI 提供商</Label>
              <Select v-model="enrichProvider" :disabled="enriching">
                <SelectTrigger class="w-[200px] h-8 text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem
                    v-for="opt in providerOptions"
                    :key="opt.value"
                    :value="opt.value"
                  >
                    {{ opt.label }}
                  </SelectItem>
                </SelectContent>
              </Select>
            </div>
            <Button size="sm" :disabled="enriching" @click="startEnrich">
              <Play class="h-3.5 w-3.5 mr-1" />
              {{ enriching ? "处理中..." : "开始丰富" }}
            </Button>
            <Button size="sm" variant="destructive" v-if="enriching" @click="stopEnrich">
              <Square class="h-3.5 w-3.5 mr-1" />
              中断
            </Button>
            <div class="flex-1 min-w-[200px]" v-if="enriching || enrichProgress > 0">
              <Progress :model-value="enrichProgress" class="h-1.5" />
              <p class="text-[11px] text-muted-foreground mt-1">{{ enrichProgressText }}</p>
            </div>
          </div>
          <p v-if="enrichError" class="text-xs text-red-500">{{ enrichError }}</p>
          <p class="text-[11px] text-muted-foreground">
            丰富后 AI 补全的元数据（含下载可靠性）会影响下方智能建议的判定
          </p>

          <!-- 待审核队列 -->
          <div v-if="reviewQueue.length > 0">
            <div class="flex items-center gap-1.5 mb-1.5">
              <Clock class="h-3.5 w-3.5 text-yellow-500" />
              <span class="text-xs font-medium">待审核（{{ reviewQueue.length }}）</span>
            </div>
            <ScrollArea class="h-[160px] border rounded-md">
              <div class="divide-y">
                <div
                  v-for="e in reviewQueue"
                  :key="e.id"
                  class="flex items-center gap-2 px-3 py-1.5"
                >
                  <div class="flex-1 min-w-0">
                    <div class="text-xs truncate">{{ e.name }}</div>
                    <div class="text-[10px] text-muted-foreground truncate">{{ e.description }}</div>
                  </div>
                  <Badge :variant="e.aiConfidence < 0.4 ? 'destructive' : 'secondary'" class="text-[9px] shrink-0">
                    {{ (e.aiConfidence * 100).toFixed(0) }}%
                  </Badge>
                  <div class="flex items-center gap-0.5 shrink-0">
                    <Button variant="ghost" size="icon" class="h-6 w-6 text-green-600" @click="acceptEntry(e.id)" title="接受">
                      <CheckCircle2 class="h-3.5 w-3.5" />
                    </Button>
                    <Button variant="ghost" size="icon" class="h-6 w-6" @click="openEdit(e)" title="编辑">
                      <Pencil class="h-3 w-3" />
                    </Button>
                    <Button variant="ghost" size="icon" class="h-6 w-6 text-destructive" @click="rejectEntry(e.id)" title="拒绝">
                      <X class="h-3 w-3" />
                    </Button>
                  </div>
                </div>
              </div>
            </ScrollArea>
          </div>
        </div>
      </CardContent>
    </Card>

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
                :checked="checkedIds.has(item.file_id)"
                @change="toggleCheck(item.file_id)"
                class="rounded"
              />
              <component :is="getSuggestionIcon(item.suggestion)" class="h-4 w-4 text-muted-foreground shrink-0" />
              <div class="flex-1 min-w-0">
                <div class="text-sm truncate">{{ item.file_name }}</div>
                <div class="text-xs text-muted-foreground truncate">{{ item.reason }}</div>
                <div v-if="item.suggestion === 'move' && item.move_target" class="text-[10px] text-primary truncate">
                  → {{ item.move_target }}
                </div>
              </div>
              <div class="text-xs text-muted-foreground shrink-0">{{ formatSize(item.file_size) }}</div>
              <Badge variant="outline" class="shrink-0 text-[10px]">
                {{ getSuggestionLabel(item.suggestion) }}
              </Badge>
              <a
                v-if="item.homepage_url"
                :href="normalizeUrl(item.homepage_url)"
                target="_blank"
                rel="noopener noreferrer"
                class="text-xs text-primary hover:underline shrink-0"
              >
                官网
              </a>
            </div>
          </div>
        </div>
      </div>
    </ScrollArea>

    <!-- 确认对话框 -->
    <div
      v-if="showConfirm"
      class="absolute inset-0 z-50 flex items-center justify-center bg-black/40"
      @click.self="showConfirm = false"
    >
      <div class="bg-card border rounded-lg shadow-lg p-5 w-[90%] max-w-md">
        <div class="flex items-center gap-2 mb-3">
          <AlertCircle class="h-5 w-5 text-yellow-500" />
          <h4 class="font-semibold">确认执行清理</h4>
        </div>
        <p class="text-sm text-muted-foreground mb-4">
          即将处理 <b>{{ checkedIds.size }}</b> 个项目。
          <span v-if="hasMoveItems">其中含<b>整目录迁移</b>（不可通过回收站恢复，请确认目标路径）。</span>
          <span v-else>删除项移至回收站，可恢复。</span>
          确定继续吗？
        </p>
        <div class="flex justify-end gap-2">
          <Button variant="outline" size="sm" @click="showConfirm = false">取消</Button>
          <Button variant="default" size="sm" @click="confirmClean">
            <Trash2 class="h-4 w-4 mr-1" />
            确认清理
          </Button>
        </div>
      </div>
    </div>

    <!-- 丰富结果编辑弹窗 -->
    <Dialog v-model:open="editOpen">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>编辑丰富结果</DialogTitle>
          <DialogDescription>{{ editItem?.name }}</DialogDescription>
        </DialogHeader>
        <div class="space-y-3">
          <div class="space-y-1">
            <Label class="text-xs">描述</Label>
            <Input v-model="editForm.description" />
          </div>
          <div class="space-y-1">
            <Label class="text-xs">功能分类</Label>
            <Input v-model="editForm.functionalCategory" />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" size="sm" @click="editOpen = false">取消</Button>
          <Button size="sm" @click="saveEdit">保存并接受</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
