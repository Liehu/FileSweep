<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { listen, type UnlistenFn } from "@/lib/api";
import { open } from "@tauri-apps/plugin-dialog";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import { ColumnFilter } from "@/components/ui/column-filter";
import { Empty } from "@/components/ui/empty";
import { Search, FolderPlus, RefreshCw, Trash2, Play, ChevronLeft, ChevronRight, Globe, X, Loader2, ListCollapse, List } from "lucide-vue-next";

const route = useRoute();
const router = useRouter();
const store = useFilesStore();
const unlisteners = ref<UnlistenFn[]>([]);

const batchAction = ref("delete");
const batchMoveTarget = ref("");
// 紧凑模式：隐藏次要列 + 压缩行高，单页显示更多项目
const compact = ref(true);

const pageTitle = computed(() => {
  if (route.query.dup) return "重复文件";
  if (route.query.mv) return "多版本文件";
  if (route.query.cat) return String(route.query.cat);
  return "全部文件";
});

const statusFilter = computed(() => {
  if (route.query.dup) return "duplicate";
  if (route.query.mv) return "multiversion";
  return undefined;
});

const categoryFilter = computed(() => {
  if (route.query.cat) return String(route.query.cat);
  return undefined;
});

const dirTypeFilter = computed(() => {
  if (route.query.dtype) return String(route.query.dtype);
  return undefined;
});

// 列筛选：从当前页数据提取唯一值
const categoryOptions = computed(() =>
  Array.from(new Set(store.files.map((f) => f.category).filter(Boolean))).sort()
);
const dirTypeOptions = computed(() =>
  Array.from(new Set(store.files.map((f) => f.appDirReason).filter(Boolean))).sort()
);
const funcCategoryOptions = computed(() =>
  Array.from(new Set(store.files.map((f) => f.functionalCategory).filter(Boolean))).sort()
);

// 目录类型 reason → 中文标签（与后端 DirType::label_from_reason 对齐）
const DIR_TYPE_LABELS: Record<string, string> = {
  CODE_PROJECT: "代码项目",
  NOTE_COLLECTION: "笔记",
  YAML_LIBRARY: "YAML库",
  CTF_CHALLENGE: "CTF题目",
  KNOWLEDGE_BASE: "知识库",
  SAMPLE_COLLECTION: "样本集合",
  TRAINING_MATERIAL: "培训资料",
  VULN_MATERIAL: "漏洞资料",
  DOC_COLLECTION: "文档集合",
  TEMP_FILES: "临时文件",
  APP_DIR: "应用目录",
  UNKNOWN: "未识别",
  "exe-app": "应用目录",
  "jar-app": "Java应用",
  "python-project": "Python项目",
  "software_root": "软件目录",
};
// 从 v2 建议摘要构建 id→建议标签映射，供建议操作 badge 显示
const suggestionMap = computed<Record<string, string>>(() => {
  const s = store.suggestionSummary;
  if (!s) return {};
  const map: Record<string, string> = {};
  const labelOf = (sug: string) => {
    const m: Record<string, string> = {
      delete: "删除", delete_old: "删旧版", delete_dup: "删重复",
      downgrade: "降级", move: "迁移",
    };
    return m[sug] || sug;
  };
  for (const it of [
    ...(s.high_confidence || []),
    ...(s.medium_confidence || []),
    ...(s.old_versions || []),
    ...(s.duplicates || []),
  ]) {
    if (it.file_id) map[it.file_id] = labelOf(it.suggestion);
  }
  return map;
});

function dirTypeLabel(reason: string): string {
  if (!reason) return "";
  return DIR_TYPE_LABELS[reason] || reason;
}
// 功能分类多选（客户端筛当前页）
const funcCategoryColumnFilter = ref<string[]>([]);
const visibleFiles = computed(() => {
  if (funcCategoryColumnFilter.value.length === 0) return store.files;
  return store.files.filter((f) => funcCategoryColumnFilter.value.includes(f.functionalCategory));
});

const categoryColorMap: Record<string, string> = {
  "安装包": "bg-blue-100 text-blue-700",
  "文档": "bg-green-100 text-green-700",
  "压缩包": "bg-orange-100 text-orange-700",
  "脚本": "bg-purple-100 text-purple-700",
  "视频": "bg-red-100 text-red-700",
  "音频": "bg-pink-100 text-pink-700",
  "图片": "bg-yellow-100 text-yellow-700",
};

function getCategoryColor(cat: string) {
  return categoryColorMap[cat] || "bg-gray-100 text-gray-700";
}

function formatSize(bytes: number) {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  return (bytes / Math.pow(1024, i)).toFixed(i > 0 ? 1 : 0) + " " + units[i];
}

async function selectDirectory() {
  const selected = await open({ directory: true, multiple: false });
  if (selected) {
    await store.startScan([selected as string]);
  }
}

async function loadFiles() {
  await store.fetchFiles(categoryFilter.value, statusFilter.value, dirTypeFilter.value);
}

async function handleBatchAction() {
  if (!store.hasSelection) return;
  await store.batchSetAction(batchAction.value, batchMoveTarget.value || undefined);
  await loadFiles();
}

function goPage(p: number) {
  if (p < 1 || p > store.totalPages) return;
  store.page = p;
  loadFiles();
}

watch(() => route.query, () => {
  store.page = 1;
  store.clearSelection();
  loadFiles();
}, { deep: true });

watch(() => store.searchQuery, () => {
  store.page = 1;
  loadFiles();
});

// ── Everything 全局搜索（防抖）──
const everythingInput = ref("");
let everythingTimer: ReturnType<typeof setTimeout> | null = null;
function onEverythingInput(v: string) {
  everythingInput.value = v;
  if (everythingTimer) clearTimeout(everythingTimer);
  everythingTimer = setTimeout(() => {
    store.searchEverything(v);
  }, 400);
}

onMounted(async () => {
  await store.fetchStats();
  await loadFiles();
  // v2 建议引擎（含重复/多版本/临时/降级），供「执行清理」和建议操作 badge 使用
  await store.fetchSuggestionsV2();

  const un1 = await listen("clean_complete", () => {
    loadFiles();
    store.fetchStats();
    store.fetchSuggestionsV2();
  });
  const un2 = await listen("clean_error", () => {});
  const un3 = await listen("scan_complete", () => {
    loadFiles();
    store.fetchStats();
    store.fetchSuggestionsV2();
  });
  unlisteners.value = [un1, un2, un3];
});

onUnmounted(() => {
  unlisteners.value.forEach((fn) => fn());
});
</script>

<template>
  <div class="p-6 space-y-4">
    <!-- Toolbar -->
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <h1 class="text-xl font-bold">{{ pageTitle }}</h1>
        <Badge variant="secondary">{{ store.total }} 个文件</Badge>
      </div>
      <div class="flex items-center gap-2">
        <Button variant="outline" size="sm" @click="selectDirectory">
          <FolderPlus class="h-4 w-4 mr-1" />
          选择目录
        </Button>
        <Button variant="outline" size="sm" @click="loadFiles">
          <RefreshCw class="h-4 w-4 mr-1" />
          刷新
        </Button>
        <Button
          :variant="compact ? 'default' : 'outline'"
          size="sm"
          @click="compact = !compact"
          :title="compact ? '当前紧凑视图，点击切换详细' : '当前详细视图，点击切换紧凑'"
        >
          <component :is="compact ? ListCollapse : List" class="h-4 w-4 mr-1" />
          {{ compact ? "紧凑" : "详细" }}
        </Button>
        <Button variant="destructive" size="sm" @click="store.executeCleanupFromSuggestions()">
          <Play class="h-4 w-4 mr-1" />
          执行清理
        </Button>
      </div>
    </div>

    <!-- Stats Summary -->
    <div class="grid grid-cols-4 gap-3">
      <Card>
        <CardContent class="pt-3 pb-3 flex items-center gap-2">
          <span class="text-lg font-bold">{{ store.stats.total }}</span>
          <span class="text-xs text-muted-foreground">总文件</span>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-3 pb-3 flex items-center gap-2">
          <span class="text-lg font-bold text-orange-600">{{ store.stats.duplicates }}</span>
          <span class="text-xs text-muted-foreground">重复</span>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-3 pb-3 flex items-center gap-2">
          <span class="text-lg font-bold text-purple-600">{{ store.stats.multiversion }}</span>
          <span class="text-xs text-muted-foreground">多版本</span>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-3 pb-3 flex items-center gap-2">
          <span class="text-lg font-bold text-gray-600">{{ store.stats.uncategorized }}</span>
          <span class="text-xs text-muted-foreground">未分类</span>
        </CardContent>
      </Card>
    </div>

    <!-- Batch Action Bar -->
    <div v-if="store.hasSelection" class="flex items-center gap-3 p-3 bg-muted rounded-lg">
      <span class="text-sm">已选择 {{ store.selectedIds.size }} 个文件</span>
      <Select v-model="batchAction">
        <SelectTrigger class="w-[150px] h-8 text-xs">
          <SelectValue placeholder="批量操作" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="delete">删除</SelectItem>
          <SelectItem value="move">移动</SelectItem>
          <SelectItem value="keep">保留</SelectItem>
          <SelectItem value="skip">跳过</SelectItem>
        </SelectContent>
      </Select>
      <Input
        v-if="batchAction === 'move'"
        v-model="batchMoveTarget"
        placeholder="目标路径"
        class="h-8 w-[200px] text-xs"
      />
      <Button size="sm" @click="handleBatchAction">应用</Button>
      <Button size="sm" variant="ghost" @click="store.clearSelection()">取消</Button>
    </div>

    <!-- Search -->
    <div class="flex items-center gap-2">
      <div class="relative flex-1 max-w-[300px]">
        <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
        <Input
          v-model="store.searchQuery"
          placeholder="搜索文件名..."
          class="pl-9 h-9"
        />
      </div>
    </div>

    <!-- Everything 全局搜索（全系统，需安装 ES CLI） -->
    <Card>
      <CardContent class="p-3 space-y-2">
        <div class="flex items-center gap-2">
          <Globe class="h-4 w-4 text-muted-foreground shrink-0" />
          <span class="text-xs font-medium shrink-0">全局搜索</span>
          <div class="relative flex-1 max-w-[400px]">
            <Input
              :model-value="everythingInput"
              @update:model-value="onEverythingInput"
              placeholder="用 Everything 搜索全盘文件（如 *.iso python.exe）..."
              class="h-8 pl-3 pr-8 text-xs"
            />
            <button
              v-if="everythingInput"
              class="absolute right-2 top-1/2 -translate-y-1/2 text-muted-foreground hover:text-foreground"
              @click="everythingInput = ''; store.clearEverythingSearch()"
            >
              <X class="h-3.5 w-3.5" />
            </button>
          </div>
          <Loader2 v-if="store.everythingSearching" class="h-4 w-4 animate-spin text-muted-foreground" />
          <Badge v-if="store.everythingSource === 'everything'" variant="secondary" class="text-[10px] text-green-600">
            Everything
          </Badge>
          <Badge v-else-if="store.everythingSource === 'database'" variant="outline" class="text-[10px]">
            数据库回退
          </Badge>
          <span v-if="store.everythingResults.length > 0" class="text-xs text-muted-foreground">
            {{ store.everythingResults.length }} 个结果
          </span>
        </div>
        <p v-if="store.everythingError" class="text-xs text-red-500">{{ store.everythingError }}</p>
        <p v-if="store.everythingSource === 'database' && everythingInput" class="text-[11px] text-muted-foreground">
          未检测到 Everything（ES CLI），已回退到已扫描数据库内搜索。安装 Everything + ES 后可获得全盘结果。
        </p>
        <!-- 结果列表 -->
        <ScrollArea v-if="store.everythingResults.length > 0" class="h-[240px] border rounded-md">
          <div class="divide-y">
            <div
              v-for="(item, idx) in store.everythingResults"
              :key="idx"
              class="flex items-center gap-2 px-3 py-1.5 hover:bg-accent/50"
            >
              <span class="text-sm truncate flex-1" :title="item.path">{{ item.name }}</span>
              <span class="text-[11px] text-muted-foreground truncate max-w-[300px]" :title="item.path">{{ item.path }}</span>
              <span class="text-[11px] text-muted-foreground shrink-0">{{ formatSize(item.size) }}</span>
            </div>
          </div>
        </ScrollArea>
      </CardContent>
    </Card>

    <!-- File Table -->
    <Card>
      <Table :class="compact ? 'compact-table' : ''">
        <TableHeader>
          <TableRow>
            <TableHead class="w-[40px]">
              <Checkbox
                :model-value="store.selectedIds.size === store.files.length && store.files.length > 0"
                @update:model-value="store.toggleSelectAll()"
              />
            </TableHead>
            <TableHead>文件名</TableHead>
            <TableHead>
              <div class="inline-flex items-center gap-1">
                分类
                <ColumnFilter
                  :options="categoryOptions"
                  :model-value="categoryFilter ? [categoryFilter] : []"
                  :single="true"
                  @update:model-value="(v) => router.push({ query: v.length ? { cat: v[0] } : {} })"
                />
              </div>
            </TableHead>
            <TableHead>
              <div class="inline-flex items-center gap-1">
                目录类型
                <ColumnFilter
                  :options="dirTypeOptions"
                  :model-value="dirTypeFilter ? [dirTypeFilter] : []"
                  :single="true"
                  @update:model-value="(v) => router.push({ query: { ...route.query, dtype: v.length ? v[0] : undefined } })"
                />
              </div>
            </TableHead>
            <TableHead v-if="!compact">
              <div class="inline-flex items-center gap-1">
                功能分类
                <ColumnFilter
                  :options="funcCategoryOptions"
                  v-model="funcCategoryColumnFilter"
                />
              </div>
            </TableHead>
            <TableHead v-if="!compact">版本</TableHead>
            <TableHead class="text-right">大小</TableHead>
            <TableHead>建议操作</TableHead>
            <TableHead>操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="store.files.length === 0">
            <TableCell :colspan="compact ? 7 : 9" class="h-48">
              <Empty :icon="Search" message="暂无文件数据" />
            </TableCell>
          </TableRow>
          <TableRow v-for="file in visibleFiles" :key="file.id">
            <TableCell>
              <Checkbox
                :model-value="store.selectedIds.has(file.id)"
                @update:model-value="store.toggleSelect(file.id)"
              />
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1.5 max-w-[250px]">
                <Badge
                  v-if="file.isAppDir"
                  variant="outline"
                  class="text-[10px] shrink-0"
                  :title="file.appDirReason"
                >
                  {{ dirTypeLabel(file.appDirReason) || "DIR" }}
                </Badge>
                <span class="truncate text-sm" :title="file.name">{{ file.name }}</span>
              </div>
            </TableCell>
            <TableCell>
              <Badge v-if="file.category" :class="getCategoryColor(file.category)" variant="secondary" class="text-[10px]">
                {{ file.category }}
              </Badge>
            </TableCell>
            <TableCell class="text-xs text-muted-foreground">
              <span v-if="file.appDirReason" :title="file.appDirReason">
                {{ dirTypeLabel(file.appDirReason) }}
              </span>
              <span v-else>-</span>
            </TableCell>
            <TableCell v-if="!compact" class="text-sm text-muted-foreground">{{ file.functionalCategory || "-" }}</TableCell>
            <TableCell v-if="!compact" class="text-sm text-muted-foreground">{{ file.version || "-" }}</TableCell>
            <TableCell class="text-right text-sm">{{ formatSize(file.fileSize) }}</TableCell>
            <TableCell>
              <Badge v-if="suggestionMap[file.id]" variant="outline" class="text-[10px]">
                {{ suggestionMap[file.id] }}
              </Badge>
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1.5">
                <!-- 扫描器已填 move_target 的目录（来自目录模式规则）→ 显示迁移标记 -->
                <template v-if="file.move_target && !file.action">
                  <Badge variant="default" class="text-[10px]" :title="file.move_target">迁移</Badge>
                  <span class="text-[10px] text-muted-foreground truncate max-w-[100px]" :title="file.move_target">
                    → {{ file.move_target }}
                  </span>
                </template>
                <!-- 普通文件：用户手动选择操作 -->
                <template v-else>
                  <Select
                    :model-value="file.action || ''"
                    @update:model-value="(v: string) => store.setAction(file.id, v)"
                  >
                    <SelectTrigger class="w-[80px] h-7 text-xs">
                      <SelectValue placeholder="选择" />
                    </SelectTrigger>
                    <SelectContent>
                      <SelectItem value="keep">保留</SelectItem>
                      <SelectItem value="delete">删除</SelectItem>
                      <SelectItem value="move">移动</SelectItem>
                      <SelectItem value="skip">跳过</SelectItem>
                    </SelectContent>
                  </Select>
                  <Input
                    v-if="file.action === 'move'"
                    :model-value="file.move_target || ''"
                    placeholder="目标"
                    class="h-7 w-[120px] text-xs"
                    @update:model-value="(v: string) => store.setMoveTarget(file.id, v)"
                  />
                </template>
              </div>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </Card>

    <!-- Pagination -->
    <div class="flex items-center justify-between text-sm">
      <span class="text-muted-foreground">
        第 {{ store.page }}/{{ store.totalPages || 1 }} 页，共 {{ store.total }} 条
      </span>
      <div class="flex items-center gap-1">
        <Button variant="outline" size="sm" :disabled="store.page <= 1" @click="goPage(store.page - 1)">
          <ChevronLeft class="h-4 w-4" />
        </Button>
        <Button
          v-for="p in Math.min(store.totalPages, 7)"
          :key="p"
          :variant="store.page === p ? 'default' : 'outline'"
          size="sm"
          class="w-8 h-8 p-0 text-xs"
          @click="goPage(p)"
        >
          {{ p }}
        </Button>
        <Button variant="outline" size="sm" :disabled="store.page >= store.totalPages" @click="goPage(store.page + 1)">
          <ChevronRight class="h-4 w-4" />
        </Button>
      </div>
    </div>
  </div>
</template>

<style scoped>
/* 紧凑表格：压缩行高 + 字号，单页显示更多项目 */
.compact-table :deep(td),
.compact-table :deep(th) {
  padding-top: 0.25rem;
  padding-bottom: 0.25rem;
  font-size: 0.75rem;
  line-height: 1.1;
}
.compact-table :deep(tbody tr) {
  height: auto;
}
</style>
