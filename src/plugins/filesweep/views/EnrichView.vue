<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed } from "vue";
import { invoke, listen, type UnlistenFn } from "@/lib/api";
import { useCatalogStore } from "@plugins/filesweep/stores/catalog";
import { useSettingsStore } from "@plugins/filesweep/stores/settings";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import { Progress } from "@/components/ui/progress";
import { Empty } from "@/components/ui/empty";
import {
  Dialog, DialogContent, DialogHeader, DialogFooter, DialogTitle, DialogDescription,
} from "@/components/ui/dialog";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Sparkles, Play, Check, Pencil, X, Clock } from "lucide-vue-next";

interface EnrichResult {
  id: number;
  file_name: string;
  category: string;
  description: string;
  confidence: number;
  status: string;
}

const catalogStore = useCatalogStore();
const settingsStore = useSettingsStore();
const provider = ref("offline");
const hasCustomProvider = computed(() => {
  const ai = settingsStore.config.ai;
  return !!(ai.custom_name && ai.custom_base_url);
});
const enriching = ref(false);
const progress = ref(0);
const progressText = ref("");
const results = ref<EnrichResult[]>([]);
const reviewQueue = computed(() => results.value.filter((r) => r.confidence < 0.6 && r.status !== "rejected"));
const enrichedCount = computed(() => results.value.filter((r) => r.status === "accepted" || r.confidence >= 0.6).length);
const needsReview = computed(() => reviewQueue.value.length);

const editOpen = ref(false);
const editItem = ref<EnrichResult | null>(null);
const editForm = ref({ description: "", category: "", tags: "" });

const unlisteners = ref<UnlistenFn[]>([]);
let pollTimer: ReturnType<typeof setInterval> | null = null;

async function startEnrich() {
  enriching.value = true;
  progress.value = 0;
  progressText.value = "准备中...";
  try {
    await invoke("start_enrich", { provider: provider.value });
    pollTimer = setInterval(() => {
      catalogStore.fetchCatalog();
    }, 3000);
  } catch (e) {
    enriching.value = false;
    progressText.value = String(e);
  }
}

async function acceptResult(item: EnrichResult) {
  item.status = "accepted";
  await catalogStore.fetchCatalog();
}

async function rejectResult(item: EnrichResult) {
  item.status = "rejected";
  await catalogStore.fetchCatalog();
}

function openEdit(item: EnrichResult) {
  editItem.value = item;
  editForm.value = { description: item.description, category: item.category, tags: "" };
  editOpen.value = true;
}

async function saveEdit() {
  if (!editItem.value) return;
  editItem.value.description = editForm.value.description;
  editItem.value.category = editForm.value.category;
  editItem.value.status = "accepted";
  editOpen.value = false;
  await catalogStore.fetchCatalog();
}

function formatConfidence(val: number) {
  return (val * 100).toFixed(1) + "%";
}

onMounted(async () => {
  // 从 catalog store 获取已有的丰富结果
  await catalogStore.fetchCatalog();
  await settingsStore.fetchSettings();

  const un1 = await listen("enrich_progress", (e: any) => {
    const p = e.payload;
    progress.value = p.total > 0 ? Math.round((p.done / p.total) * 100) : 0;
    progressText.value = `${p.done}/${p.total} - ${p.currentName}`;
  });
  const un2 = await listen("enrich_complete", () => {
    enriching.value = false;
    progress.value = 100;
    progressText.value = "完成";
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
    catalogStore.fetchCatalog();
  });
  const un3 = await listen<string>("enrich_error", (e) => {
    enriching.value = false;
    progressText.value = e.payload;
    if (pollTimer) { clearInterval(pollTimer); pollTimer = null; }
  });
  unlisteners.value = [un1, un2, un3];
});

onUnmounted(() => {
  if (pollTimer) clearInterval(pollTimer);
  unlisteners.value.forEach((fn) => fn());
});
</script>

<template>
  <div class="p-6 space-y-4">
    <div class="flex items-center gap-2">
      <Sparkles class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">AI 元数据丰富</h1>
    </div>

    <!-- Provider + Start -->
    <Card>
      <CardContent class="pt-4 pb-4">
        <div class="flex items-center gap-4">
          <div class="space-y-1">
            <Label>AI 提供商</Label>
            <Select v-model="provider" :disabled="enriching">
              <SelectTrigger class="w-[200px]">
                <SelectValue placeholder="选择提供商" />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="offline">离线规则</SelectItem>
                <SelectItem value="ollama">Ollama</SelectItem>
                <SelectItem value="openai">OpenAI</SelectItem>
                <SelectItem value="claude">Claude</SelectItem>
                <SelectItem v-if="hasCustomProvider" value="custom">自定义 ({{ settingsStore.config.ai.custom_name }})</SelectItem>
              </SelectContent>
            </Select>
          </div>
          <div class="flex-1">
            <Button :disabled="enriching" @click="startEnrich">
              <Play class="h-4 w-4 mr-2" />
              {{ enriching ? '处理中...' : '开始丰富' }}
            </Button>
          </div>
          <div class="flex-1 space-y-1" v-if="enriching || progress > 0">
            <Progress :model-value="progress" class="h-2" />
            <p class="text-xs text-muted-foreground">{{ progressText }}</p>
          </div>
        </div>
      </CardContent>
    </Card>

    <!-- Stats -->
    <div class="grid grid-cols-2 gap-3">
      <Card>
        <CardContent class="pt-3 pb-3 flex items-center gap-3">
          <div class="p-2 rounded-lg bg-green-100 text-green-600">
            <Check class="h-4 w-4" />
          </div>
          <div>
            <p class="text-lg font-bold">{{ enrichedCount }}</p>
            <p class="text-xs text-muted-foreground">已丰富</p>
          </div>
        </CardContent>
      </Card>
      <Card>
        <CardContent class="pt-3 pb-3 flex items-center gap-3">
          <div class="p-2 rounded-lg bg-yellow-100 text-yellow-600">
            <Clock class="h-4 w-4" />
          </div>
          <div>
            <p class="text-lg font-bold">{{ needsReview }}</p>
            <p class="text-xs text-muted-foreground">待审核</p>
          </div>
        </CardContent>
      </Card>
    </div>

    <!-- Tabs: Review + All -->
    <Tabs default-value="review">
      <TabsList>
        <TabsTrigger value="review">
          待审核 ({{ needsReview }})
        </TabsTrigger>
        <TabsTrigger value="all">
          全部结果 ({{ results.length }})
        </TabsTrigger>
      </TabsList>

      <TabsContent value="review">
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>文件名</TableHead>
                <TableHead>分类</TableHead>
                <TableHead>描述</TableHead>
                <TableHead>置信度</TableHead>
                <TableHead class="w-[120px]">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-if="reviewQueue.length === 0">
                <TableCell :colspan="5" class="h-32">
                  <Empty message="没有需要审核的结果" />
                </TableCell>
              </TableRow>
              <TableRow v-for="item in reviewQueue" :key="item.id">
                <TableCell class="text-sm font-medium">{{ item.file_name }}</TableCell>
                <TableCell>
                  <Badge variant="secondary" class="text-[10px]">{{ item.category }}</Badge>
                </TableCell>
                <TableCell class="text-sm text-muted-foreground max-w-[200px] truncate">
                  {{ item.description }}
                </TableCell>
                <TableCell>
                  <Badge :variant="item.confidence < 0.4 ? 'destructive' : 'secondary'" class="text-[10px]">
                    {{ formatConfidence(item.confidence) }}
                  </Badge>
                </TableCell>
                <TableCell>
                  <div class="flex items-center gap-1">
                    <Button variant="outline" size="sm" class="h-7 text-xs" @click="acceptResult(item)">
                      <Check class="h-3 w-3 mr-1" />接受
                    </Button>
                    <Button variant="ghost" size="icon" class="h-7 w-7" @click="openEdit(item)">
                      <Pencil class="h-3 w-3" />
                    </Button>
                    <Button variant="ghost" size="icon" class="h-7 w-7 text-destructive" @click="rejectResult(item)">
                      <X class="h-3 w-3" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Card>
      </TabsContent>

      <TabsContent value="all">
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>文件名</TableHead>
                <TableHead>分类</TableHead>
                <TableHead>描述</TableHead>
                <TableHead>置信度</TableHead>
                <TableHead>状态</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-if="results.length === 0">
                <TableCell :colspan="5" class="h-32">
                  <Empty message="暂无丰富结果" />
                </TableCell>
              </TableRow>
              <TableRow v-for="item in results" :key="item.id">
                <TableCell class="text-sm">{{ item.file_name }}</TableCell>
                <TableCell>
                  <Badge variant="secondary" class="text-[10px]">{{ item.category }}</Badge>
                </TableCell>
                <TableCell class="text-sm text-muted-foreground truncate max-w-[250px]">
                  {{ item.description }}
                </TableCell>
                <TableCell class="text-xs">{{ formatConfidence(item.confidence) }}</TableCell>
                <TableCell>
                  <Badge
                    :variant="item.status === 'accepted' ? 'default' : item.status === 'rejected' ? 'destructive' : 'secondary'"
                    class="text-[10px]"
                  >
                    {{ item.status === 'accepted' ? '已接受' : item.status === 'rejected' ? '已拒绝' : '待审核' }}
                  </Badge>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Card>
      </TabsContent>
    </Tabs>

    <!-- Edit Dialog -->
    <Dialog v-model:open="editOpen">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>编辑丰富结果</DialogTitle>
          <DialogDescription>{{ editItem?.file_name }}</DialogDescription>
        </DialogHeader>
        <div class="space-y-4">
          <div class="space-y-1">
            <Label>描述</Label>
            <Textarea v-model="editForm.description" :rows="3" />
          </div>
          <div class="space-y-1">
            <Label>分类</Label>
            <Input v-model="editForm.category" />
          </div>
          <div class="space-y-1">
            <Label>标签（逗号分隔）</Label>
            <Input v-model="editForm.tags" placeholder="tag1, tag2" />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="editOpen = false">取消</Button>
          <Button @click="saveEdit">保存并接受</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
