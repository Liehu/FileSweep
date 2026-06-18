<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { invoke } from "@/lib/api";
import Papa from "papaparse";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/ui/badge";
import { Checkbox } from "@/components/ui/checkbox";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import { Empty } from "@/components/ui/empty";
import { ScrollText, Search, RotateCcw, Download, X } from "lucide-vue-next";

interface LogEntry {
  id: number;
  timestamp: string;
  operation: string;
  source_path: string;
  dest_path: string;
  reason: string;
  file_hash: string;
  file_size: number;
  status: string;
  session_id: string;
  can_revert: boolean;
}

const logs = ref<LogEntry[]>([]);
const loading = ref(false);
const searchQuery = ref("");
const actionFilter = ref("");
const statusFilter = ref("");
const selectedIds = ref<Set<number>>(new Set());

const statusColorMap: Record<string, string> = {
  success: "bg-green-100 text-green-700",
  error: "bg-red-100 text-red-700",
  warning: "bg-yellow-100 text-yellow-700",
  info: "bg-blue-100 text-blue-700",
};

const actionBadgeMap: Record<string, string> = {
  delete: "destructive",
  move: "default",
  scan: "secondary",
  categorize: "secondary",
  clean: "default",
  enrich: "secondary",
  revert: "outline",
};

function getStatusColor(status: string) {
  return statusColorMap[status] || "bg-gray-100 text-gray-700";
}

function getActionBadge(action: string): any {
  return actionBadgeMap[action] || "secondary";
}

async function fetchLogs() {
  loading.value = true;
  try {
    const res = await invoke<any>("get_logs", {
      page: 1,
      pageSize: 1000,
      q: searchQuery.value || undefined,
      action: actionFilter.value || undefined,
      status: statusFilter.value || undefined,
    });
    logs.value = res.logs || res.items || [];
  } catch (e) {
    console.error(e);
  } finally {
    loading.value = false;
  }
}

async function revertLog(id: number) {
  try {
    await invoke("revert_log", { id });
    await fetchLogs();
  } catch (e) {
    console.error(e);
  }
}

async function batchRevert() {
  if (selectedIds.value.size === 0) return;
  try {
    await invoke("batch_revert_logs", { ids: Array.from(selectedIds.value) });
    selectedIds.value = new Set();
    await fetchLogs();
  } catch (e) {
    console.error(e);
  }
}

function toggleSelect(id: number) {
  if (selectedIds.value.has(id)) {
    selectedIds.value.delete(id);
  } else {
    selectedIds.value.add(id);
  }
  selectedIds.value = new Set(selectedIds.value);
}

function resetFilters() {
  searchQuery.value = "";
  actionFilter.value = "";
  statusFilter.value = "";
  selectedIds.value = new Set();
  fetchLogs();
}

function exportCsv() {
  const data = logs.value.map((l) => ({
    时间: l.timestamp,
    操作: l.operation,
    源路径: l.source_path,
    目标路径: l.dest_path,
    原因: l.reason,
    状态: l.status,
  }));
  const csv = Papa.unparse(data);
  const blob = new Blob(["\uFEFF" + csv], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = "logs.csv";
  a.click();
  URL.revokeObjectURL(url);
}

onMounted(fetchLogs);
</script>

<template>
  <div class="p-6 space-y-4">
    <div class="flex items-center gap-2">
      <ScrollText class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">操作日志</h1>
      <Badge variant="secondary">{{ logs.length }}</Badge>
    </div>

    <!-- Filters -->
    <Card>
      <CardContent class="pt-3 pb-3">
        <div class="flex items-center gap-3">
          <div class="relative flex-1 max-w-[250px]">
            <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
            <Input v-model="searchQuery" placeholder="搜索日志..." class="pl-9 h-8" @keydown.enter="fetchLogs" />
          </div>
          <Select v-model="actionFilter">
            <SelectTrigger class="w-[130px] h-8 text-xs">
              <SelectValue placeholder="操作类型" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="delete">删除</SelectItem>
              <SelectItem value="move">移动</SelectItem>
              <SelectItem value="scan">扫描</SelectItem>
              <SelectItem value="categorize">分类</SelectItem>
              <SelectItem value="clean">清理</SelectItem>
              <SelectItem value="enrich">丰富</SelectItem>
            </SelectContent>
          </Select>
          <Select v-model="statusFilter">
            <SelectTrigger class="w-[100px] h-8 text-xs">
              <SelectValue placeholder="状态" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="success">成功</SelectItem>
              <SelectItem value="error">失败</SelectItem>
              <SelectItem value="warning">警告</SelectItem>
              <SelectItem value="info">信息</SelectItem>
            </SelectContent>
          </Select>
          <Button variant="outline" size="sm" @click="resetFilters">
            <X class="h-3 w-3 mr-1" /> 重置
          </Button>
          <div class="flex-1" />
          <Button v-if="selectedIds.size > 0" variant="destructive" size="sm" @click="batchRevert">
            <RotateCcw class="h-3 w-3 mr-1" /> 批量撤销 ({{ selectedIds.size }})
          </Button>
          <Button variant="outline" size="sm" @click="exportCsv">
            <Download class="h-3 w-3 mr-1" /> 导出 CSV
          </Button>
        </div>
      </CardContent>
    </Card>

    <!-- Log Table -->
    <Card>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead class="w-[40px]"></TableHead>
            <TableHead class="w-[180px]">时间</TableHead>
            <TableHead class="w-[100px]">操作</TableHead>
            <TableHead>详情</TableHead>
            <TableHead class="w-[80px]">状态</TableHead>
            <TableHead class="w-[80px]">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="logs.length === 0">
            <TableCell :colspan="6" class="h-48">
              <Empty :icon="ScrollText" message="暂无操作日志" />
            </TableCell>
          </TableRow>
          <TableRow v-for="log in logs" :key="log.id">
            <TableCell>
              <Checkbox
                v-if="log.can_revert"
                :model-value="selectedIds.has(log.id)"
                @update:model-value="toggleSelect(log.id)"
              />
            </TableCell>
            <TableCell class="text-xs text-muted-foreground">
              {{ log.timestamp?.replace("T", " ").split(".")[0] || "-" }}
            </TableCell>
            <TableCell>
              <Badge :variant="getActionBadge(log.operation)" class="text-[10px]">
                {{ log.operation }}
              </Badge>
            </TableCell>
            <TableCell class="text-sm max-w-[400px] truncate" :title="log.reason">
              {{ log.reason || log.source_path || '-' }}
            </TableCell>
            <TableCell>
              <span :class="[getStatusColor(log.status), 'inline-flex px-2 py-0.5 rounded-full text-[10px] font-medium']">
                {{ { success: '成功', error: '失败', warning: '警告', info: '信息' }[log.status] || log.status }}
              </span>
            </TableCell>
            <TableCell>
              <Button
                v-if="log.can_revert"
                variant="ghost"
                size="sm"
                class="h-7 text-xs"
                @click="revertLog(log.id)"
              >
                <RotateCcw class="h-3 w-3 mr-1" /> 撤销
              </Button>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </Card>
  </div>
</template>
