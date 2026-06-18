<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Progress } from "@/components/ui/progress";
import { Badge } from "@/components/ui/badge";
import { Scan, Plus, X, FolderSearch, FileCheck, FolderSearch as FolderIcon, Clock, Folder } from "lucide-vue-next";

const store = useFilesStore();

const dirs = ref<string[]>([]);
const newDir = ref("");
const excludeDirs = ref("");
const excludeNames = ref("");
const excludeExts = ref("");
const detectAppDirs = ref(false);

const elapsed = ref(0);
let timer: ReturnType<typeof setInterval> | null = null;
const unlisteners = ref<UnlistenFn[]>([]);

const progressPercent = computed(() => {
  if (!store.scanProgress) return 0;
  const { total, done } = store.scanProgress;
  if (total <= 0) return 0;
  return Math.min(Math.round((done / total) * 100), 100);
});

const elapsedFormatted = computed(() => {
  const total = elapsed.value;
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
});

function addDir() {
  const d = newDir.value.trim();
  if (d && !dirs.value.includes(d)) {
    dirs.value.push(d);
    newDir.value = "";
  }
}

function removeDir(index: number) {
  dirs.value.splice(index, 1);
}

function handleDirKeydown(e: KeyboardEvent) {
  if (e.key === "Enter") {
    e.preventDefault();
    addDir();
  }
}

async function startScan() {
  if (dirs.value.length === 0) return;
  elapsed.value = 0;
  if (timer) clearInterval(timer);
  timer = setInterval(() => { elapsed.value++; }, 1000);
  await store.startScan(dirs.value, {
    recursive: true,
    excludeDirs: excludeDirs.value.split("\n").map((s) => s.trim()).filter(Boolean),
    excludeNames: excludeNames.value.split("\n").map((s) => s.trim()).filter(Boolean),
    excludeExts: excludeExts.value.split("\n").map((s) => s.trim()).filter(Boolean),
    detectAppDirs: detectAppDirs.value,
  });
}

onMounted(async () => {
  if (store.lastScanDir.length > 0) {
    dirs.value = [...store.lastScanDir];
  }
  const un1 = await listen("scan_progress", () => {});
  const un2 = await listen("scan_complete", () => {
    if (timer) { clearInterval(timer); timer = null; }
  });
  const un3 = await listen("scan_error", () => {
    if (timer) { clearInterval(timer); timer = null; }
  });
  unlisteners.value = [un1, un2, un3];
  await store.fetchStats();
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
  unlisteners.value.forEach((fn) => fn());
});
</script>

<template>
  <div class="p-6 space-y-6">
    <div class="flex items-center gap-2">
      <Scan class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">扫描文件</h1>
    </div>

    <Card>
      <CardHeader class="pb-3">
        <CardTitle class="text-base">扫描目录</CardTitle>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="flex gap-2">
          <Input
            v-model="newDir"
            placeholder="输入目录路径，按 Enter 添加"
            class="flex-1"
            @keydown="handleDirKeydown"
          />
          <Button variant="outline" size="sm" @click="addDir">
            <Plus class="h-4 w-4" />
          </Button>
        </div>
        <div v-if="dirs.length > 0" class="flex flex-wrap gap-2">
          <Badge v-for="(dir, i) in dirs" :key="dir" variant="secondary" class="flex items-center gap-1 pr-1">
            <Folder class="h-3 w-3" />
            <span class="max-w-[200px] truncate">{{ dir }}</span>
            <button class="ml-1 hover:text-destructive" @click="removeDir(i)">
              <X class="h-3 w-3" />
            </button>
          </Badge>
        </div>
      </CardContent>
    </Card>

    <Card>
      <CardHeader class="pb-3">
        <CardTitle class="text-base">排除规则</CardTitle>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="grid grid-cols-3 gap-3">
          <div class="space-y-1">
            <Label class="text-xs">排除目录（每行一个）</Label>
            <Textarea v-model="excludeDirs" placeholder="node_modules&#10;.git&#10;__pycache__" :rows="3" />
          </div>
          <div class="space-y-1">
            <Label class="text-xs">排除文件名（每行一个）</Label>
            <Textarea v-model="excludeNames" placeholder="Thumbs.db&#10;.DS_Store" :rows="3" />
          </div>
          <div class="space-y-1">
            <Label class="text-xs">排除扩展名（每行一个）</Label>
            <Textarea v-model="excludeExts" placeholder=".tmp&#10;.log" :rows="3" />
          </div>
        </div>
        <div class="flex items-center gap-2">
          <Switch v-model="detectAppDirs" />
          <Label>识别绿色软件目录</Label>
        </div>
      </CardContent>
    </Card>

    <div class="flex items-center gap-3">
      <Button :disabled="dirs.length === 0 || store.scanState === 'scanning'" @click="startScan">
        <Scan class="h-4 w-4 mr-2" />
        {{ store.scanState === 'scanning' ? '扫描中...' : '开始扫描' }}
      </Button>
    </div>

    <!-- Progress Card -->
    <Card v-if="store.scanState === 'scanning' || store.scanProgress">
      <CardHeader class="pb-3">
        <CardTitle class="text-base flex items-center gap-2">
          <FolderSearch class="h-4 w-4" />
          扫描进度
        </CardTitle>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="flex items-center justify-between text-sm">
          <span>{{ store.scanProgress?.stage || store.scanProgress?.currentFile || '准备中...' }}</span>
          <div class="flex items-center gap-2 text-muted-foreground">
            <Clock class="h-3 w-3" />
            {{ elapsedFormatted }}
          </div>
        </div>
        <Progress :model-value="progressPercent" class="h-2" />
        <p v-if="store.scanProgress?.currentFile" class="text-xs text-muted-foreground truncate">
          {{ store.scanProgress.currentFile }}
        </p>
        <p class="text-xs text-muted-foreground">
          已扫描 {{ store.scanProgress?.done ?? 0 }} 个文件
          <span v-if="store.scanProgress?.total">
            （共 {{ store.scanProgress.total }} 个）
          </span>
        </p>
      </CardContent>
    </Card>

    <!-- Stats Cards -->
    <div class="grid grid-cols-4 gap-4">
      <Card>
        <CardContent class="pt-4 pb-4">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-lg bg-blue-100 text-blue-600">
              <FileCheck class="h-4 w-4" />
            </div>
            <div>
              <p class="text-2xl font-bold">{{ store.stats.total }}</p>
              <p class="text-xs text-muted-foreground">总文件数</p>
            </div>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-4 pb-4">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-lg bg-orange-100 text-orange-600">
              <FolderIcon class="h-4 w-4" />
            </div>
            <div>
              <p class="text-2xl font-bold">{{ store.stats.duplicates }}</p>
              <p class="text-xs text-muted-foreground">重复文件</p>
            </div>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-4 pb-4">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-lg bg-purple-100 text-purple-600">
              <Folder class="h-4 w-4" />
            </div>
            <div>
              <p class="text-2xl font-bold">{{ store.stats.multiversion }}</p>
              <p class="text-xs text-muted-foreground">多版本文件</p>
            </div>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-4 pb-4">
          <div class="flex items-center gap-3">
            <div class="p-2 rounded-lg bg-gray-100 text-gray-600">
              <FolderSearch class="h-4 w-4" />
            </div>
            <div>
              <p class="text-2xl font-bold">{{ store.stats.uncategorized }}</p>
              <p class="text-xs text-muted-foreground">未分类</p>
            </div>
          </div>
        </CardContent>
      </Card>
    </div>

    <!-- Error state -->
    <Card v-if="store.scanState === 'error'" class="border-destructive">
      <CardContent class="pt-4 pb-4">
        <p class="text-sm text-destructive">扫描出错: {{ store.error }}</p>
      </CardContent>
    </Card>
  </div>
</template>
