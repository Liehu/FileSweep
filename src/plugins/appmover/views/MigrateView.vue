<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";
import { useAppMoverStore, type CandidateDir } from "@plugins/appmover/stores/appmover";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Checkbox } from "@/components/ui/checkbox";
import { Progress } from "@/components/ui/progress";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Dialog, DialogContent, DialogHeader, DialogFooter, DialogTitle, DialogDescription,
} from "@/components/ui/dialog";
import { HardDriveDownload, RefreshCw, FolderInput, AlertTriangle, CheckCircle2, Sparkles } from "lucide-vue-next";

const store = useAppMoverStore();

const selected = ref<Set<string>>(new Set());
const busy = ref(false);
const progressInfo = ref<{ stage: string; copied: number; total: number; msg: string } | null>(null);
let unlisten: UnlistenFn | null = null;

// 目标根映射编辑
const newSource = ref("");
const newTarget = ref("");

// 迁移计划对话框
const planOpen = ref(false);
const planTarget = ref<{ cand: CandidateDir; src: string; dst: string; size: number; files: number; free: number; ok: boolean } | null>(null);
const killResult = ref<{ safe: boolean; message: string } | null>(null);
const confirmOpen = ref(false);
const countdown = ref(0);
let countdownTimer: ReturnType<typeof setInterval> | null = null;

function fmtBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

const selectedCands = computed(() => store.candidates.filter((c) => selected.value.has(c.path)));

async function refresh() {
  await store.scanCandidates();
  await store.fetchTargetMap();
}

/** 对单个候选调用 AI 补全描述（命中离线知识库则填充） */
async function describeWithAi(cand: CandidateDir) {
  try {
    const d = await store.describe(cand.name);
    if (d.source === "ai") {
      // 更新本地候选的描述显示
      const c = store.candidates.find((x) => x.path === cand.path);
      if (c) {
        c.description = d.description;
        c.software_name = d.software_name;
      }
    } else {
      alert("AI 未命中该目录的描述（离线知识库无此条目）");
    }
  } catch (e) {
    alert("AI 描述失败：" + String(e));
  }
}

function toggle(path: string, checked: boolean) {
  if (checked) selected.value.add(path);
  else selected.value.delete(path);
  selected.value = new Set(selected.value);
}

async function addTargetMap() {
  if (!newSource.value || !newTarget.value) return;
  await store.setTargetMap(newSource.value, newTarget.value);
  newSource.value = "";
  newTarget.value = "";
}

async function removeTargetMap(src: string) {
  await store.removeTargetMap(src);
}

/** 规划单个候选的迁移：解析目标路径 + 锁定检测 */
async function planOne(cand: CandidateDir) {
  busy.value = true;
  try {
    const plan = await store.planMigration(cand.path);
    planTarget.value = {
      cand,
      src: plan.source_path,
      dst: plan.target_path,
      size: plan.size_bytes,
      files: plan.file_count,
      free: plan.target_free_bytes,
      ok: plan.space_ok && plan.locks.safe,
    };
    killResult.value = null;
    planOpen.value = true;
  } catch (e) {
    alert("规划失败：" + String(e));
  } finally {
    busy.value = false;
  }
}

/** 关闭占用进程（三级） */
async function doKill() {
  if (!planTarget.value) return;
  busy.value = true;
  try {
    const r = await store.killLocks(planTarget.value.src, false);
    killResult.value = { safe: r.safe, message: r.message };
    // 复检 plan
    const plan = await store.planMigration(planTarget.value.src);
    planTarget.value.ok = plan.space_ok && plan.locks.safe;
  } catch (e) {
    alert("关闭占用失败：" + String(e));
  } finally {
    busy.value = false;
  }
}

/** 弹出 10s 倒计时确认 */
function startConfirm() {
  if (!planTarget.value) return;
  confirmOpen.value = true;
  countdown.value = 10;
  countdownTimer = setInterval(() => {
    countdown.value -= 1;
    if (countdown.value <= 0) {
      if (countdownTimer) clearInterval(countdownTimer);
      countdownTimer = null;
    }
  }, 1000);
}

function cancelConfirm() {
  confirmOpen.value = false;
  if (countdownTimer) clearInterval(countdownTimer);
  countdownTimer = null;
}

/** 确认后执行迁移（方案 P） */
async function doMigrate() {
  if (!planTarget.value) return;
  confirmOpen.value = false;
  if (countdownTimer) clearInterval(countdownTimer);
  busy.value = true;
  progressInfo.value = { stage: "copying", copied: 0, total: planTarget.value.files, msg: "准备迁移" };
  try {
    const r = await store.executeMigration(planTarget.value.src, planTarget.value.dst);
    progressInfo.value = null;
    alert(`迁移完成：${r.file_count} 个文件`);
    planOpen.value = false;
    await refresh();
    await store.fetchJobs();
  } catch (e) {
    progressInfo.value = null;
    alert("迁移失败：" + String(e) + "\n（C: 原件保留，可重试续传）");
  } finally {
    busy.value = false;
  }
}

onMounted(async () => {
  await refresh();
  unlisten = await listen<any>("am:migrate_progress", (e) => {
    const p = e.payload;
    progressInfo.value = { stage: p.stage, copied: p.copied, total: p.total, msg: p.message };
  });
});

import { onUnmounted } from "vue";
onUnmounted(() => {
  if (unlisten) unlisten();
  if (countdownTimer) clearInterval(countdownTimer);
});

const progressPct = computed(() => {
  if (!progressInfo.value || !progressInfo.value.total) return 0;
  return Math.min(Math.round((progressInfo.value.copied / progressInfo.value.total) * 100), 100);
});
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-semibold flex items-center gap-2">
          <HardDriveDownload class="w-5 h-5" /> 目录迁移
        </h2>
        <p class="text-sm text-muted-foreground">
          将 AppData / Program Files 下的非系统默认目录迁移到目标盘，并用 Junction 保证原应用正常使用
        </p>
      </div>
      <Button variant="outline" @click="refresh" :disabled="store.loading">
        <RefreshCw class="w-4 h-4 mr-1" :class="{ 'animate-spin': store.loading }" /> 刷新
      </Button>
    </div>

    <!-- 目标根映射 -->
    <Card>
      <CardHeader>
        <CardTitle class="text-base flex items-center gap-2"><FolderInput class="w-4 h-4" /> 迁移目标根映射</CardTitle>
        <CardDescription>配置源根 → 目标根，迁移时自动解析目标路径</CardDescription>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="flex gap-2 items-end">
          <div class="flex-1 space-y-1">
            <Label class="text-xs">源根（如 C:\Users\X\AppData\Roaming）</Label>
            <Input v-model="newSource" placeholder="C:\Users\X\AppData\Roaming" />
          </div>
          <div class="flex-1 space-y-1">
            <Label class="text-xs">目标根（如 D:\Users\X\AppData\Roaming）</Label>
            <Input v-model="newTarget" placeholder="D:\Users\X\AppData\Roaming" />
          </div>
          <Button @click="addTargetMap">添加</Button>
        </div>
        <div v-if="store.targetMap.length" class="space-y-1">
          <div v-for="m in store.targetMap" :key="m.source_root" class="flex items-center justify-between text-sm border rounded px-3 py-2">
            <span class="font-mono text-xs">{{ m.source_root }} <span class="text-muted-foreground">→</span> {{ m.target_root }}</span>
            <Button variant="ghost" size="sm" @click="removeTargetMap(m.source_root)">移除</Button>
          </div>
        </div>
      </CardContent>
    </Card>

    <!-- 进度条 -->
    <Card v-if="progressInfo">
      <CardContent class="pt-4 space-y-2">
        <div class="flex justify-between text-sm">
          <span>{{ progressInfo.msg }}</span>
          <span>{{ progressInfo.copied }}/{{ progressInfo.total }}</span>
        </div>
        <Progress :model-value="progressPct" />
      </CardContent>
    </Card>

    <!-- 候选列表 -->
    <Card>
      <CardHeader>
        <CardTitle class="text-base">候选迁移目录（{{ store.candidates.length }}）</CardTitle>
        <CardDescription>已过滤系统默认目录（强白名单 ∪ 基线）。点击右侧按钮规划单个迁移</CardDescription>
      </CardHeader>
      <CardContent>
        <ScrollArea class="h-[480px] pr-3">
          <div class="space-y-1">
            <div v-for="c in store.candidates" :key="c.path" class="flex items-center gap-3 border rounded px-3 py-2 hover:bg-accent">
              <Checkbox :model-value="selected.has(c.path)" @update:model-value="(v) => toggle(c.path, !!v)" />
              <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2">
                  <span class="font-medium truncate">{{ c.name }}</span>
                  <Badge v-if="c.is_junction" variant="secondary" class="text-xs">已迁移</Badge>
                  <Badge v-if="c.software_name && c.software_name !== c.name" variant="outline" class="text-xs">{{ c.software_name }}</Badge>
                </div>
                <div class="text-xs text-muted-foreground font-mono truncate">{{ c.path }}</div>
                <div v-if="c.description" class="text-xs text-muted-foreground">{{ c.description }}</div>
              </div>
              <div class="text-right text-xs text-muted-foreground whitespace-nowrap">
                <div>{{ fmtBytes(c.size_bytes) }}</div>
                <div>{{ c.file_count }} 文件</div>
              </div>
              <Button size="sm" variant="ghost" @click="describeWithAi(c)" :disabled="busy" title="AI 补全描述">
                <Sparkles class="w-3 h-3" />
              </Button>
              <Button size="sm" @click="planOne(c)" :disabled="busy || c.is_junction">规划</Button>
            </div>
            <div v-if="!store.candidates.length" class="text-center text-sm text-muted-foreground py-8">
              暂无候选目录，点击右上角刷新
            </div>
          </div>
        </ScrollArea>
      </CardContent>
    </Card>

    <!-- 规划对话框 -->
    <Dialog v-model:open="planOpen">
      <DialogContent class="max-w-2xl">
        <DialogHeader>
          <DialogTitle>迁移规划</DialogTitle>
          <DialogDescription>方案 P：复制 → 校验 → 建 Junction → 删源</DialogDescription>
        </DialogHeader>
        <div v-if="planTarget" class="space-y-3">
          <div class="text-sm space-y-1 font-mono bg-muted p-3 rounded">
            <div>源：{{ planTarget.src }}</div>
            <div class="text-muted-foreground">↓</div>
            <div>标：{{ planTarget.dst }}</div>
          </div>
          <div class="grid grid-cols-3 gap-2 text-sm">
            <div class="border rounded p-2"><div class="text-xs text-muted-foreground">大小</div><div>{{ fmtBytes(planTarget.size) }}</div></div>
            <div class="border rounded p-2"><div class="text-xs text-muted-foreground">文件数</div><div>{{ planTarget.files }}</div></div>
            <div class="border rounded p-2"><div class="text-xs text-muted-foreground">目标盘剩余</div><div>{{ fmtBytes(planTarget.free) }}</div></div>
          </div>
          <div v-if="killResult" class="flex items-center gap-2 text-sm" :class="killResult.safe ? 'text-green-600' : 'text-orange-600'">
            <component :is="killResult.safe ? CheckCircle2 : AlertTriangle" class="w-4 h-4" />
            {{ killResult.message }}
          </div>
          <div class="flex items-center gap-2 text-sm" v-if="!planTarget.ok">
            <AlertTriangle class="w-4 h-4 text-orange-600" />
            <span v-if="!planTarget.ok">目标盘空间不足或目录被占用，请先关闭占用进程</span>
          </div>
        </div>
        <DialogFooter class="gap-2">
          <Button variant="outline" @click="doKill" :disabled="busy">关闭占用进程</Button>
          <Button variant="outline" @click="planOpen = false">取消</Button>
          <Button @click="startConfirm" :disabled="busy || !planTarget?.ok" class="gap-1">
            <HardDriveDownload class="w-4 h-4" /> 执行迁移
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- 倒计时确认 -->
    <Dialog v-model:open="confirmOpen">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>确认执行迁移</DialogTitle>
          <DialogDescription>
            迁移将复制全部文件并建立目录链接。请确保相关应用已关闭、未保存文档已保存。
          </DialogDescription>
        </DialogHeader>
        <div class="text-center py-4">
          <div class="text-4xl font-bold">{{ countdown }}</div>
          <div class="text-sm text-muted-foreground mt-2">秒后可执行</div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="cancelConfirm">取消</Button>
          <Button @click="doMigrate" :disabled="countdown > 0">立即执行</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
