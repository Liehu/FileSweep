<script setup lang="ts">
import { ref, computed, watch, onMounted } from "vue";
import { useAppMoverStore } from "@plugins/appmover/stores/appmover";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Tabs, TabsList, TabsTrigger, TabsContent,
} from "@/components/ui/tabs";
import { Variable, Download, Upload, Database } from "lucide-vue-next";

const store = useAppMoverStore();
const scope = ref<"user" | "system">("user");
const installedSearch = ref("");

function fmtTime(ts: number): string {
  if (!ts) return "-";
  return new Date(ts * 1000).toLocaleString();
}

async function backup() {
  if (!confirm(`确认备份 ${scope.value === "user" ? "用户" : "系统"} 环境变量到数据库？`)) return;
  try {
    const r = await store.backupEnv(scope.value);
    alert(`已备份 ${r.backed_up} 个变量`);
    await store.fetchEnvBackups(scope.value);
  } catch (e) {
    alert("备份失败：" + String(e));
  }
}

async function restore(backedUpAt: number) {
  if (!confirm(`确认把该备份写回 ${scope.value === "user" ? "用户" : "系统"} 环境变量？\n系统变量写入需管理员权限。`)) return;
  try {
    const r = await store.restoreEnv(scope.value, backedUpAt);
    alert(`已恢复 ${r.restored} 个变量\n注意：新进程才会感知变化`);
  } catch (e) {
    alert("恢复失败：" + String(e));
  }
}

function groupByTime(entries: { id: number; backed_up_at: number; key: string; value: string }[]) {
  const map = new Map<number, { key: string; value: string }[]>();
  for (const e of entries) {
    if (!map.has(e.backed_up_at)) map.set(e.backed_up_at, []);
    map.get(e.backed_up_at)!.push({ key: e.key, value: e.value });
  }
  return Array.from(map.entries()).map(([time, items]) => ({ time, items })).sort((a, b) => b.time - a.time);
}

const scopeEntries = computed(() => store.envBackups.filter((e) => e.scope === scope.value));
const groups = computed(() => groupByTime(scopeEntries.value));

const filteredInstalled = computed(() => {
  if (!installedSearch.value) return store.installed;
  const q = installedSearch.value.toLowerCase();
  return store.installed.filter((i) => i.name.toLowerCase().includes(q) || i.publisher.toLowerCase().includes(q));
});

watch(scope, () => store.fetchEnvBackups(scope.value));

onMounted(async () => {
  await store.fetchEnvBackups(scope.value);
  await store.fetchInstalled();
});
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-xl font-semibold flex items-center gap-2"><Variable class="w-5 h-5" /> 环境变量</h2>
      <p class="text-sm text-muted-foreground">备份/恢复用户与系统环境变量；只读查看已安装程序列表</p>
    </div>

    <Tabs v-model="scope">
      <TabsList>
        <TabsTrigger value="user">用户环境变量</TabsTrigger>
        <TabsTrigger value="system">系统环境变量</TabsTrigger>
      </TabsList>

      <TabsContent :value="scope">
        <Card>
          <CardHeader>
            <div class="flex items-center justify-between">
              <div>
                <CardTitle class="text-base">{{ scope === "user" ? "用户" : "系统" }}环境变量备份</CardTitle>
                <CardDescription>备份组按时间展示，可整组恢复</CardDescription>
              </div>
              <Button @click="backup"><Download class="w-4 h-4 mr-1" /> 备份当前</Button>
            </div>
          </CardHeader>
          <CardContent>
            <ScrollArea class="h-[400px] pr-3">
              <div class="space-y-2">
                <div v-for="g in groups" :key="g.time" class="border rounded p-2">
                  <div class="flex items-center justify-between mb-1">
                    <span class="text-sm font-medium">{{ fmtTime(g.time) }}</span>
                    <div class="flex items-center gap-2">
                      <Badge variant="secondary">{{ g.items.length }} 个变量</Badge>
                      <Button size="sm" variant="outline" @click="restore(g.time)">
                        <Upload class="w-3 h-3 mr-1" /> 恢复此组
                      </Button>
                    </div>
                  </div>
                  <div class="space-y-1">
                    <div v-for="(item, idx) in g.items" :key="idx" class="text-xs font-mono">
                      <span class="text-muted-foreground">{{ item.key }}=</span>
                      <span class="truncate">{{ item.value }}</span>
                    </div>
                  </div>
                </div>
                <div v-if="!groups.length" class="text-center text-sm text-muted-foreground py-8">
                  暂无备份，点击右上角"备份当前"
                </div>
              </div>
            </ScrollArea>
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>

    <Card>
      <CardHeader>
        <CardTitle class="text-base flex items-center gap-2"><Database class="w-4 h-4" /> 已安装程序（只读）</CardTitle>
        <CardDescription>来自 Uninstall 注册表，仅展示，不做备份/恢复</CardDescription>
      </CardHeader>
      <CardContent>
        <Input v-model="installedSearch" placeholder="搜索软件名或发行商..." class="mb-3" />
        <ScrollArea class="h-[320px] pr-3">
          <div class="space-y-1">
            <div v-for="i in filteredInstalled" :key="i.name" class="flex items-center justify-between text-sm border rounded px-3 py-1.5">
              <div class="min-w-0">
                <span class="font-medium">{{ i.name }}</span>
                <span v-if="i.version" class="text-xs text-muted-foreground ml-2">{{ i.version }}</span>
              </div>
              <span class="text-xs text-muted-foreground truncate ml-2">{{ i.publisher }}</span>
            </div>
            <div v-if="!filteredInstalled.length" class="text-center text-sm text-muted-foreground py-4">无匹配</div>
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  </div>
</template>
