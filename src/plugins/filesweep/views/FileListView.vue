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
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import { Empty } from "@/components/ui/empty";
import { Search, FolderPlus, RefreshCw, Trash2, Play, ChevronLeft, ChevronRight } from "lucide-vue-next";

const route = useRoute();
const router = useRouter();
const store = useFilesStore();
const unlisteners = ref<UnlistenFn[]>([]);

const batchAction = ref("delete");
const batchMoveTarget = ref("");

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
  await store.fetchFiles(categoryFilter.value, statusFilter.value);
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

onMounted(async () => {
  await store.fetchStats();
  await loadFiles();
  await store.fetchSuggestions();

  const un1 = await listen("clean_complete", () => { loadFiles(); store.fetchStats(); });
  const un2 = await listen("clean_error", () => {});
  const un3 = await listen("scan_complete", () => { loadFiles(); store.fetchStats(); });
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
        <Button variant="destructive" size="sm" @click="store.executeCleanup()">
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

    <!-- File Table -->
    <Card>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead class="w-[40px]">
              <Checkbox
                :model-value="store.selectedIds.size === store.files.length && store.files.length > 0"
                @update:model-value="store.toggleSelectAll()"
              />
            </TableHead>
            <TableHead>文件名</TableHead>
            <TableHead>分类</TableHead>
            <TableHead>功能分类</TableHead>
            <TableHead>版本</TableHead>
            <TableHead class="text-right">大小</TableHead>
            <TableHead>建议操作</TableHead>
            <TableHead>操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="store.files.length === 0">
            <TableCell :colspan="8" class="h-48">
              <Empty :icon="Search" message="暂无文件数据" />
            </TableCell>
          </TableRow>
          <TableRow v-for="file in store.files" :key="file.id">
            <TableCell>
              <Checkbox
                :model-value="store.selectedIds.has(file.id)"
                @update:model-value="store.toggleSelect(file.id)"
              />
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1.5 max-w-[250px]">
                <Badge v-if="file.isAppDir" variant="outline" class="text-[10px] shrink-0">DIR</Badge>
                <span class="truncate text-sm" :title="file.name">{{ file.name }}</span>
              </div>
            </TableCell>
            <TableCell>
              <Badge v-if="file.category" :class="getCategoryColor(file.category)" variant="secondary" class="text-[10px]">
                {{ file.category }}
              </Badge>
            </TableCell>
            <TableCell class="text-sm text-muted-foreground">{{ file.functionalCategory || "-" }}</TableCell>
            <TableCell class="text-sm text-muted-foreground">{{ file.version || "-" }}</TableCell>
            <TableCell class="text-right text-sm">{{ formatSize(file.fileSize) }}</TableCell>
            <TableCell>
              <Badge v-if="store.suggestions[file.id]" variant="outline" class="text-[10px]">
                {{ store.suggestions[file.id] }}
              </Badge>
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1.5">
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
