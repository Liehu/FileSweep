<script setup lang="ts">
import { ref, onMounted, computed, watch } from "vue";
import { invoke } from "@/lib/api";
import { useCatalogStore } from "@plugins/filesweep/stores/catalog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import {
  Dialog, DialogContent, DialogHeader, DialogFooter, DialogTitle, DialogDescription, DialogTrigger,
} from "@/components/ui/dialog";
import { Empty } from "@/components/ui/empty";
import { BookOpen, Search, Download, Pencil, Trash2, ExternalLink } from "lucide-vue-next";

const store = useCatalogStore();
const editOpen = ref(false);
const editItem = ref<any>(null);
const exportOpen = ref(false);

const editForm = ref({
  description: "",
  functionalCategory: "",
  latestVersion: "",
  homepageUrl: "",
  downloadUrl: "",
  license: "",
  tags: "",
});

async function openEdit(entry: any) {
  editItem.value = entry;
  editForm.value = {
    description: entry.description || "",
    functionalCategory: entry.functionalCategory || "",
    latestVersion: entry.latestVersion || "",
    homepageUrl: entry.homepageUrl || "",
    downloadUrl: entry.downloadUrl || "",
    license: entry.license || "",
    tags: (entry.tags || []).join(", "),
  };
  editOpen.value = true;
}

async function saveEdit() {
  if (!editItem.value) return;
  await store.updateEntry(editItem.value.id, {
    description: editForm.value.description,
    functionalCategory: editForm.value.functionalCategory,
    latestVersion: editForm.value.latestVersion,
    homepageUrl: editForm.value.homepageUrl,
    downloadUrl: editForm.value.downloadUrl,
    license: editForm.value.license,
    tags: editForm.value.tags.split(",").map((s) => s.trim()).filter(Boolean),
  });
  editOpen.value = false;
}

async function handleDelete(entry: any) {
  if (confirm(`确认删除 ${entry.name} ?`)) {
    await store.deleteEntry(entry.id);
  }
}

async function handleExportCsv() {
  const content = await store.exportCsv();
  const blob = new Blob([content], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "catalog.csv";
  a.click();
  URL.revokeObjectURL(url);
  exportOpen.value = false;
}

async function handleExportMd() {
  const content = await store.exportObsidianMd();
  const blob = new Blob([content], { type: "text/markdown;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "catalog.md";
  a.click();
  URL.revokeObjectURL(url);
  exportOpen.value = false;
}

watch(() => store.searchQuery, () => {
  store.page = 1;
  store.fetchCatalog();
});

onMounted(() => {
  store.fetchCatalog();
});
</script>

<template>
  <div class="p-6 space-y-4">
    <div class="flex items-center justify-between">
      <div class="flex items-center gap-2">
        <BookOpen class="h-5 w-5 text-primary" />
        <h1 class="text-xl font-bold">软件目录</h1>
        <Badge variant="secondary">{{ store.total }}</Badge>
      </div>
      <div class="flex items-center gap-2">
        <div class="relative">
          <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input v-model="store.searchQuery" placeholder="搜索软件..." class="pl-9 w-[250px] h-9" />
        </div>
        <Dialog v-model:open="exportOpen">
          <DialogTrigger as-child>
            <Button variant="outline" size="sm">
              <Download class="h-4 w-4 mr-1" />
              导出
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>导出目录</DialogTitle>
              <DialogDescription>选择导出格式</DialogDescription>
            </DialogHeader>
            <div class="flex gap-3">
              <Button class="flex-1" @click="handleExportCsv">导出 CSV</Button>
              <Button class="flex-1" variant="outline" @click="handleExportMd">导出 Obsidian MD</Button>
            </div>
          </DialogContent>
        </Dialog>
      </div>
    </div>

    <Card>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>名称</TableHead>
            <TableHead>功能分类</TableHead>
            <TableHead>描述</TableHead>
            <TableHead>标签</TableHead>
            <TableHead>链接</TableHead>
            <TableHead>版本</TableHead>
            <TableHead>更新时间</TableHead>
            <TableHead class="w-[100px]">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="store.entries.length === 0">
            <TableCell :colspan="8" class="h-48">
              <Empty :icon="BookOpen" message="暂无目录数据" />
            </TableCell>
          </TableRow>
          <TableRow v-for="entry in store.entries" :key="entry.id">
            <TableCell class="font-medium">{{ entry.name }}</TableCell>
            <TableCell>
              <Badge v-if="entry.functionalCategory" variant="secondary" class="text-[10px]">
                {{ entry.functionalCategory }}
              </Badge>
            </TableCell>
            <TableCell class="max-w-[200px] truncate text-sm text-muted-foreground">
              {{ entry.description || "-" }}
            </TableCell>
            <TableCell>
              <div class="flex flex-wrap gap-1">
                <Badge v-for="tag in (entry.tags || []).slice(0, 3)" :key="tag" variant="outline" class="text-[10px]">
                  {{ tag }}
                </Badge>
                <Badge v-if="(entry.tags || []).length > 3" variant="outline" class="text-[10px]">
                  +{{ entry.tags.length - 3 }}
                </Badge>
              </div>
            </TableCell>
            <TableCell>
              <div class="flex gap-1">
                <a v-if="entry.homepageUrl" :href="entry.homepageUrl" target="_blank" class="text-primary hover:underline text-xs flex items-center gap-0.5">
                  <ExternalLink class="h-3 w-3" /> 主页
                </a>
                <a v-if="entry.downloadUrl" :href="entry.downloadUrl" target="_blank" class="text-primary hover:underline text-xs flex items-center gap-0.5">
                  <ExternalLink class="h-3 w-3" /> 下载
                </a>
              </div>
            </TableCell>
            <TableCell class="text-sm text-muted-foreground">{{ entry.latestVersion || "-" }}</TableCell>
            <TableCell class="text-xs text-muted-foreground">{{ entry.metaUpdatedAt?.split("T")[0] || "-" }}</TableCell>
            <TableCell>
              <div class="flex items-center gap-1">
                <Button variant="ghost" size="icon" class="h-7 w-7" @click="openEdit(entry)">
                  <Pencil class="h-3.5 w-3.5" />
                </Button>
                <Button variant="ghost" size="icon" class="h-7 w-7 text-destructive" @click="handleDelete(entry)">
                  <Trash2 class="h-3.5 w-3.5" />
                </Button>
              </div>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </Card>

    <!-- Pagination -->
    <div class="flex items-center justify-between text-sm">
      <span class="text-muted-foreground">第 {{ store.page }}/{{ store.totalPages || 1 }} 页</span>
      <div class="flex gap-1">
        <Button variant="outline" size="sm" :disabled="store.page <= 1" @click="store.page--; store.fetchCatalog()">
          上一页
        </Button>
        <Button variant="outline" size="sm" :disabled="store.page >= store.totalPages" @click="store.page++; store.fetchCatalog()">
          下一页
        </Button>
      </div>
    </div>

    <!-- Edit Dialog -->
    <Dialog v-model:open="editOpen">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>编辑软件信息</DialogTitle>
          <DialogDescription>{{ editItem?.name }}</DialogDescription>
        </DialogHeader>
        <div class="space-y-4">
          <div class="space-y-1">
            <Label>描述</Label>
            <Textarea v-model="editForm.description" :rows="3" placeholder="软件描述..." />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1">
              <Label>功能分类</Label>
              <Input v-model="editForm.functionalCategory" placeholder="如：开发工具" />
            </div>
            <div class="space-y-1">
              <Label>最新版本</Label>
              <Input v-model="editForm.latestVersion" placeholder="如：1.0.0" />
            </div>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1">
              <Label>主页地址</Label>
              <Input v-model="editForm.homepageUrl" placeholder="https://..." />
            </div>
            <div class="space-y-1">
              <Label>下载地址</Label>
              <Input v-model="editForm.downloadUrl" placeholder="https://..." />
            </div>
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1">
              <Label>许可证</Label>
              <Input v-model="editForm.license" placeholder="MIT, Apache..." />
            </div>
            <div class="space-y-1">
              <Label>标签（逗号分隔）</Label>
              <Input v-model="editForm.tags" placeholder="tag1, tag2, tag3" />
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="editOpen = false">取消</Button>
          <Button @click="saveEdit">保存</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
