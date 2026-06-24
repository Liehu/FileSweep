<script setup lang="ts">
import { ref, computed, onMounted } from "vue";
import { useAppMoverStore } from "@plugins/appmover/stores/appmover";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { ShieldCheck, Upload, Plus, Trash2, FileUp, ScanLine } from "lucide-vue-next";

const store = useAppMoverStore();

const newProtected = ref("");
const baselineFile = ref("");
const importType = ref<"file" | "scan">("scan");

const sourceFilter = ref<"all" | "hardcoded" | "baseline" | "user">("all");

const filteredProtected = computed(() => {
  if (sourceFilter.value === "all") return store.protectedSet;
  return store.protectedSet.filter((p) => p.source === sourceFilter.value);
});

const sourceVariant = (s: string): "default" | "secondary" | "outline" => {
  if (s === "hardcoded") return "default";
  if (s === "baseline") return "secondary";
  return "outline";
};

const sourceLabel = (s: string): string => {
  if (s === "hardcoded") return "强白名单";
  if (s === "baseline") return "基线";
  if (s === "user") return "用户";
  return s;
};

async function addProtected() {
  if (!newProtected.value.trim()) return;
  await store.addProtected(newProtected.value.trim());
  newProtected.value = "";
}

async function removeProtected(name: string) {
  await store.removeProtected(name);
}

async function importBaseline() {
  try {
    if (importType.value === "file") {
      if (!baselineFile.value) {
        alert("请填写基线文件路径");
        return;
      }
      const r = await store.importBaselineFile(baselineFile.value);
      alert(`已导入 ${r.imported} 个目录到保护集`);
    } else {
      const r = await store.setFirstScanAsBaseline();
      alert(`已把当前监控根的一级目录（${r.imported} 个）作为基线导入保护集`);
    }
    await store.fetchProtected();
  } catch (e) {
    alert("导入失败：" + String(e));
  }
}

onMounted(() => store.fetchProtected());
</script>

<template>
  <div class="space-y-4">
    <div>
      <h2 class="text-xl font-semibold flex items-center gap-2"><ShieldCheck class="w-5 h-5" /> 基线管理</h2>
      <p class="text-sm text-muted-foreground">
        保护集 = 强白名单 ∪ 基线 ∪ 用户。被保护的目录永不进入候选迁移列表
      </p>
    </div>

    <Card>
      <CardHeader>
        <CardTitle class="text-base">导入基线</CardTitle>
        <CardDescription>
          基线仅用于识别系统默认目录，不做快照恢复。推荐：在纯净 VM 扫描后导出基线文件，导入本机
        </CardDescription>
      </CardHeader>
      <CardContent>
        <Tabs v-model="importType">
          <TabsList>
            <TabsTrigger value="scan"><ScanLine class="w-3 h-3 mr-1" /> 首次扫描作基线</TabsTrigger>
            <TabsTrigger value="file"><FileUp class="w-3 h-3 mr-1" /> 导入基线文件</TabsTrigger>
          </TabsList>
          <TabsContent value="scan">
            <p class="text-sm text-muted-foreground py-2">
              把当前所有监控根（AppData / Program Files）下的一级目录全部加入保护集。适合"用户当前状态"作为基线。
            </p>
            <Button @click="importBaseline"><Upload class="w-4 h-4 mr-1" /> 扫描并导入</Button>
          </TabsContent>
          <TabsContent value="file">
            <div class="flex gap-2 py-2">
              <Input v-model="baselineFile" placeholder="基线文件路径（每行一个目录名，# 注释）" class="flex-1" />
              <Button @click="importBaseline"><Upload class="w-4 h-4 mr-1" /> 导入</Button>
            </div>
            <p class="text-xs text-muted-foreground">建议从纯净 VM 导出，避免把用户数据当作基线</p>
          </TabsContent>
        </Tabs>
      </CardContent>
    </Card>

    <Card>
      <CardHeader>
        <div class="flex items-center justify-between">
          <div>
            <CardTitle class="text-base">保护集（{{ store.protectedSet.length }}）</CardTitle>
            <CardDescription>硬白名单不可删除</CardDescription>
          </div>
          <div class="flex gap-1">
            <Button v-for="s in ['all', 'hardcoded', 'baseline', 'user']" :key="s"
              :variant="sourceFilter === s ? 'default' : 'outline'" size="sm"
              @click="sourceFilter = (s as any)">
              {{ s === 'all' ? '全部' : sourceLabel(s) }}
            </Button>
          </div>
        </div>
      </CardHeader>
      <CardContent class="space-y-3">
        <div class="flex gap-2">
          <Input v-model="newProtected" placeholder="添加自定义保护目录名" @keyup.enter="addProtected" />
          <Button @click="addProtected"><Plus class="w-4 h-4 mr-1" /> 添加</Button>
        </div>
        <ScrollArea class="h-[400px] pr-3">
          <div class="grid grid-cols-2 gap-1">
            <div v-for="p in filteredProtected" :key="p.path" class="flex items-center justify-between border rounded px-2 py-1 text-sm">
              <span class="font-mono text-xs truncate">{{ p.path }}</span>
              <div class="flex items-center gap-1 flex-shrink-0">
                <Badge :variant="sourceVariant(p.source)" class="text-xs">{{ sourceLabel(p.source) }}</Badge>
                <Button v-if="p.source !== 'hardcoded'" variant="ghost" size="sm" @click="removeProtected(p.path)">
                  <Trash2 class="w-3 h-3" />
                </Button>
              </div>
            </div>
          </div>
          <div v-if="!filteredProtected.length" class="text-center text-sm text-muted-foreground py-8">
            无匹配项
          </div>
        </ScrollArea>
      </CardContent>
    </Card>
  </div>
</template>
