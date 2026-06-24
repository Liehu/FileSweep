<script setup lang="ts">
import { onMounted } from "vue";
import { useAppMoverStore } from "@plugins/appmover/stores/appmover";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { History, RefreshCw, RotateCcw } from "lucide-vue-next";

const store = useAppMoverStore();

function fmtTime(ts: number | null): string {
  if (!ts) return "-";
  return new Date(ts * 1000).toLocaleString();
}

function fmtBytes(n: number): string {
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
}

function statusVariant(s: string): "default" | "secondary" | "destructive" | "outline" {
  if (s === "done") return "default";
  if (s === "failed" || s === "manual") return "destructive";
  if (s === "copying" || s === "verifying" || s === "linking") return "secondary";
  return "outline";
}

async function retry(jobId: number) {
  if (!confirm("重试该迁移任务？（已复制的文件将跳过，续传剩余）")) return;
  try {
    const r = await store.retryMigration(jobId);
    alert(`续传完成：${r.file_count} 个文件`);
    await store.fetchJobs();
  } catch (e) {
    alert("重试失败：" + String(e));
  }
}

onMounted(() => store.fetchJobs());
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-semibold flex items-center gap-2"><History class="w-5 h-5" /> 迁移历史</h2>
        <p class="text-sm text-muted-foreground">所有迁移作业记录。失败任务可重试续传</p>
      </div>
      <Button variant="outline" @click="store.fetchJobs()"><RefreshCw class="w-4 h-4 mr-1" /> 刷新</Button>
    </div>

    <Card>
      <CardContent class="pt-4">
        <ScrollArea class="h-[560px] pr-3">
          <div class="space-y-2">
            <div v-for="j in store.jobs" :key="j.id" class="border rounded p-3 space-y-1">
              <div class="flex items-center gap-2">
                <Badge :variant="statusVariant(j.status)">{{ j.status }}</Badge>
                <span class="text-xs text-muted-foreground">#{{ j.id }}</span>
                <span class="text-sm font-mono truncate flex-1">{{ j.source_path }}</span>
                <Button v-if="j.status === 'failed'" size="sm" variant="outline" @click="retry(j.id)">
                  <RotateCcw class="w-3 h-3 mr-1" /> 重试
                </Button>
              </div>
              <div class="text-xs font-mono text-muted-foreground">→ {{ j.target_path }}</div>
              <div class="flex gap-4 text-xs text-muted-foreground">
                <span>{{ j.copied_count }}/{{ j.file_count }} 文件</span>
                <span>{{ fmtBytes(j.total_bytes) }}</span>
                <span>开始：{{ fmtTime(j.started_at) }}</span>
                <span>结束：{{ fmtTime(j.finished_at) }}</span>
              </div>
              <div v-if="j.error" class="text-xs text-destructive">{{ j.error }}</div>
            </div>
            <div v-if="!store.jobs.length" class="text-center text-sm text-muted-foreground py-8">
              暂无迁移记录
            </div>
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  </div>
</template>
