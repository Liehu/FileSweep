<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { invoke } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Empty } from "@/components/ui/empty";
import { Tag, Plus, Pencil, Trash2, Save, X } from "lucide-vue-next";

interface TagItem {
  id: string;
  name: string;
  color: string;
  description: string;
  count: number;
}

const tags = ref<TagItem[]>([]);
const loading = ref(false);
const editingId = ref<string | null>(null);

const newName = ref("");
const newColor = ref("#3b82f6");
const newDesc = ref("");

const editName = ref("");
const editColor = ref("");
const editDesc = ref("");

const presetColors = [
  "#3b82f6", "#ef4444", "#22c55e", "#f59e0b", "#8b5cf6",
  "#ec4899", "#06b6d4", "#f97316", "#14b8a6", "#6366f1",
  "#84cc16", "#e11d48",
];

async function fetchTags() {
  loading.value = true;
  try {
    tags.value = await invoke<TagItem[]>("get_tags");
  } catch (e) {
    console.error(e);
  } finally {
    loading.value = false;
  }
}

async function addTag() {
  if (!newName.value.trim()) return;
  try {
    await invoke("create_tag", {
      name: newName.value.trim(),
      color: newColor.value,
      description: newDesc.value.trim(),
    });
    newName.value = "";
    newDesc.value = "";
    await fetchTags();
  } catch (e) {
    console.error(e);
  }
}

function startEdit(tag: TagItem) {
  editingId.value = tag.id;
  editName.value = tag.name;
  editColor.value = tag.color;
  editDesc.value = tag.description;
}

async function saveEdit() {
  if (editingId.value == null) return;
  try {
    await invoke("update_tag", {
      id: editingId.value,
      name: editName.value.trim(),
      color: editColor.value,
      description: editDesc.value.trim(),
    });
    editingId.value = null;
    await fetchTags();
  } catch (e) {
    console.error(e);
  }
}

async function deleteTag(id: string) {
  try {
    await invoke("delete_tag", { id });
    await fetchTags();
  } catch (e) {
    console.error(e);
  }
}

onMounted(fetchTags);
</script>

<template>
  <div class="p-6 space-y-4">
    <div class="flex items-center gap-2">
      <Tag class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">标签管理</h1>
      <Badge variant="secondary">{{ tags.length }}</Badge>
    </div>

    <!-- New Tag Form -->
    <Card>
      <CardContent class="pt-4 pb-4 space-y-3">
        <div class="flex items-end gap-3">
          <div class="space-y-1">
            <Label class="text-xs">标签名称</Label>
            <Input v-model="newName" placeholder="输入标签名称" class="w-[180px]" />
          </div>
          <div class="space-y-1">
            <Label class="text-xs">颜色</Label>
            <div class="flex items-center gap-2">
              <input type="color" v-model="newColor" class="w-8 h-8 rounded cursor-pointer border" />
              <div class="flex gap-1">
                <button
                  v-for="color in presetColors"
                  :key="color"
                  :class="[
                    'w-5 h-5 rounded-full border-2 transition-transform',
                    newColor === color ? 'border-foreground scale-110' : 'border-transparent hover:scale-110',
                  ]"
                  :style="{ backgroundColor: color }"
                  @click="newColor = color"
                />
              </div>
            </div>
          </div>
          <div class="space-y-1 flex-1">
            <Label class="text-xs">描述</Label>
            <Input v-model="newDesc" placeholder="标签描述（可选）" />
          </div>
          <Button @click="addTag">
            <Plus class="h-4 w-4 mr-1" /> 添加标签
          </Button>
        </div>
      </CardContent>
    </Card>

    <!-- Tags Table -->
    <Card>
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead class="w-[40px]">颜色</TableHead>
            <TableHead>名称</TableHead>
            <TableHead>描述</TableHead>
            <TableHead class="w-[80px]">引用数</TableHead>
            <TableHead class="w-[100px]">操作</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          <TableRow v-if="tags.length === 0">
            <TableCell :colspan="5" class="h-32">
              <Empty :icon="Tag" message="暂无标签" />
            </TableCell>
          </TableRow>
          <TableRow v-for="tag in tags" :key="tag.id">
            <TableCell>
              <div
                v-if="editingId === tag.id"
                class="w-6 h-6 rounded-full border cursor-pointer"
                :style="{ backgroundColor: editColor }"
              >
                <input type="color" v-model="editColor" class="w-full h-full opacity-0 cursor-pointer" />
              </div>
              <div v-else class="w-4 h-4 rounded-full" :style="{ backgroundColor: tag.color }" />
            </TableCell>
            <TableCell>
              <template v-if="editingId === tag.id">
                <Input v-model="editName" class="h-8 w-[150px]" />
              </template>
              <template v-else>
                <span
                  class="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full text-xs font-medium text-white"
                  :style="{ backgroundColor: tag.color }"
                >
                  {{ tag.name }}
                </span>
              </template>
            </TableCell>
            <TableCell>
              <template v-if="editingId === tag.id">
                <Input v-model="editDesc" class="h-8" />
              </template>
              <template v-else>
                <span class="text-sm text-muted-foreground">{{ tag.description || "-" }}</span>
              </template>
            </TableCell>
            <TableCell>
              <Badge variant="secondary" class="text-[10px]">{{ tag.count }}</Badge>
            </TableCell>
            <TableCell>
              <div class="flex items-center gap-1">
                <template v-if="editingId === tag.id">
                  <Button variant="outline" size="sm" class="h-7" @click="saveEdit">
                    <Save class="h-3 w-3 mr-1" /> 保存
                  </Button>
                  <Button variant="ghost" size="sm" class="h-7" @click="editingId = null">
                    <X class="h-3 w-3" />
                  </Button>
                </template>
                <template v-else>
                  <Button variant="ghost" size="icon" class="h-7 w-7" @click="startEdit(tag)">
                    <Pencil class="h-3.5 w-3.5" />
                  </Button>
                  <Button variant="ghost" size="icon" class="h-7 w-7 text-destructive" @click="deleteTag(tag.id)">
                    <Trash2 class="h-3.5 w-3.5" />
                  </Button>
                </template>
              </div>
            </TableCell>
          </TableRow>
        </TableBody>
      </Table>
    </Card>
  </div>
</template>
