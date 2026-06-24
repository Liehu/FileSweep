<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";
import { listen, type UnlistenFn } from "@/lib/api";
import { useAppMoverStore } from "@plugins/appmover/stores/appmover";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import {
  Select, SelectTrigger, SelectContent, SelectItem, SelectValue,
} from "@/components/ui/select";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Radar, Play, Square, Check, Bell, RefreshCw } from "lucide-vue-next";

const store = useAppMoverStore();
const interval = ref("1800"); // 秒，默认 30 分钟
const badgeCount = ref(0);
const autostartEnabled = ref(false);
let pollTimer: ReturnType<typeof setInterval> | null = null;
let unlisten: UnlistenFn | null = null;

const intervalOptions = [
  { label: "15 分钟", value: "900" },
  { label: "30 分钟", value: "1800" },
  { label: "1 小时", value: "3600" },
  { label: "1 天", value: "86400" },
];

function fmtTime(ts: number): string {
  if (!ts) return "-";
  return new Date(ts * 1000).toLocaleString();
}

async function start() {
  await store.startMonitor(Number(interval.value));
  pollTimer = setInterval(() => store.fetchMonitorEvents(), 10000);
}

async function stop() {
  await store.stopMonitor();
  if (pollTimer) clearInterval(pollTimer);
  pollTimer = null;
}

async function checkNow() {
  // 立即检查并刷新角标
  await store.refreshBadge();
  await store.fetchMonitorEvents();
  const b = await store.getBadge();
  badgeCount.value = b.count;
}

async function toggleAutostart(v: boolean) {
  try {
    await store.setAutostart(v);
    autostartEnabled.value = v;
  } catch (e) {
    alert("切换开机自启失败：" + String(e));
    autostartEnabled.value = !v;
  }
}

onMounted(async () => {
  await store.fetchMonitorEvents();
  const b = await store.getBadge();
  badgeCount.value = b.count;
  const a = await store.getAutostart();
  autostartEnabled.value = a.enabled;
  if (store.monitorRunning) {
    pollTimer = setInterval(() => store.fetchMonitorEvents(), 10000);
  }
  // 监听托盘角标更新事件
  unlisten = await listen<number>("am:monitor_updated", (e) => {
    badgeCount.value = e.payload;
    store.fetchMonitorEvents();
  });
});

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
  if (unlisten) unlisten();
});
</script>

<template>
  <div class="space-y-4">
    <div class="flex items-center justify-between">
      <div>
        <h2 class="text-xl font-semibold flex items-center gap-2">
          <Radar class="w-5 h-5" /> 目录监控
          <Badge v-if="badgeCount > 0" variant="destructive" class="ml-1">
            <Bell class="w-3 h-3 mr-1" /> {{ badgeCount }}
          </Badge>
        </h2>
        <p class="text-sm text-muted-foreground">轮询监控根下的一级目录增删，发现新增/卸载残留时托盘角标提醒</p>
      </div>
      <div class="flex items-center gap-2">
        <Button variant="outline" @click="checkNow"><RefreshCw class="w-4 h-4 mr-1" /> 立即检查</Button>
        <Select v-model="interval">
          <SelectTrigger class="w-32"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem v-for="o in intervalOptions" :key="o.value" :value="o.value">{{ o.label }}</SelectItem>
          </SelectContent>
        </Select>
        <Button v-if="!store.monitorRunning" @click="start"><Play class="w-4 h-4 mr-1" /> 启动</Button>
        <Button v-else variant="destructive" @click="stop"><Square class="w-4 h-4 mr-1" /> 停止</Button>
      </div>
    </div>

    <!-- 自启设置 -->
    <Card>
      <CardContent class="pt-4 flex items-center justify-between">
        <div>
          <Label class="flex items-center gap-2"><Bell class="w-4 h-4" /> 开机自启动</Label>
          <p class="text-xs text-muted-foreground mt-1">开机后自动启动并在后台轮询监控，托盘图标显示待处理角标</p>
        </div>
        <Switch :model-value="autostartEnabled" @update:model-value="toggleAutostart" />
      </CardContent>
    </Card>

    <Card>
      <CardHeader>
        <CardTitle class="text-base flex items-center gap-2">
          待处理事件
          <Badge v-if="store.monitorEvents.length" variant="destructive">{{ store.monitorEvents.length }}</Badge>
          <Badge :variant="store.monitorRunning ? 'default' : 'secondary'">
            {{ store.monitorRunning ? "运行中" : "已停止" }}
          </Badge>
        </CardTitle>
        <CardDescription>新增目录（state=new）或卸载残留（state=resident）</CardDescription>
      </CardHeader>
      <CardContent>
        <ScrollArea class="h-[500px] pr-3">
          <div class="space-y-1">
            <div v-for="e in store.monitorEvents" :key="e.watch_root + e.dir_name" class="flex items-center gap-3 border rounded px-3 py-2">
              <Badge :variant="e.state === 'new' ? 'default' : 'secondary'">{{ e.state }}</Badge>
              <div class="flex-1 min-w-0">
                <div class="font-medium truncate">{{ e.dir_name }}</div>
                <div class="text-xs text-muted-foreground font-mono truncate">{{ e.full_path }}</div>
              </div>
              <div class="text-xs text-muted-foreground text-right">
                <div>发现：{{ fmtTime(e.first_seen_at) }}</div>
                <div>最近：{{ fmtTime(e.last_seen_at) }}</div>
              </div>
              <Button size="sm" variant="ghost" @click="store.dismissEvent(e.watch_root, e.dir_name)">
                <Check class="w-4 h-4" />
              </Button>
            </div>
            <div v-if="!store.monitorEvents.length" class="text-center text-sm text-muted-foreground py-8">
              暂无待处理事件
            </div>
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  </div>
</template>
