<script setup lang="ts">
import { ref, onMounted } from "vue";
import { invoke } from "@/lib/api";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Card, CardContent } from "@/components/ui/card";
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from "@/components/ui/table";
import { Tabs, TabsList, TabsTrigger, TabsContent } from "@/components/ui/tabs";
import { Empty } from "@/components/ui/empty";
import { FolderOpen, Plus, Pencil, Trash2, X, Save } from "lucide-vue-next";

interface FuncCategory {
  name: string;
  description?: string;
  parent?: string;
}

interface FileRule {
  name: string;
  target_path: string;
  extensions: string[];
  name_keywords: string[];
}

const funcCategories = ref<FuncCategory[]>([]);
const fileRules = ref<FileRule[]>([]);
const loading = ref(false);

// Func category form
const newFuncName = ref("");
const newFuncDesc = ref("");
const editingFuncName = ref<string | null>(null);
const editFuncName = ref("");
const editFuncDesc = ref("");

// File rule form
const newRuleName = ref("");
const newRuleTarget = ref("");
const newRuleExts = ref("");
const newRuleKeywords = ref("");
const editingRuleIdx = ref<number | null>(null);
const editRuleName = ref("");
const editRuleTarget = ref("");
const editRuleExts = ref("");
const editRuleKeywords = ref("");

async function fetchFuncCategories() {
  try {
    funcCategories.value = await invoke<FuncCategory[]>("get_func_categories");
  } catch (e) {
    console.error(e);
  }
}

async function fetchFileRules() {
  try {
    const res = await invoke<any>("get_rules");
    fileRules.value = res?.categories || [];
  } catch (e) {
    console.error(e);
  }
}

async function saveFuncCategories() {
  try {
    await invoke("update_func_categories", { categories: funcCategories.value });
  } catch (e) {
    console.error(e);
  }
}

async function addFuncCategory() {
  if (!newFuncName.value.trim()) return;
  funcCategories.value.push({
    name: newFuncName.value.trim(),
    description: newFuncDesc.value.trim() || undefined,
  });
  newFuncName.value = "";
  newFuncDesc.value = "";
  await saveFuncCategories();
}

async function startEditFunc(cat: FuncCategory) {
  editingFuncName.value = cat.name;
  editFuncName.value = cat.name;
  editFuncDesc.value = cat.description || "";
}

async function saveFuncCategory() {
  if (!editingFuncName.value) return;
  const idx = funcCategories.value.findIndex((c) => c.name === editingFuncName.value);
  if (idx >= 0) {
    funcCategories.value[idx] = {
      name: editFuncName.value.trim(),
      description: editFuncDesc.value.trim() || undefined,
      parent: funcCategories.value[idx].parent,
    };
  }
  editingFuncName.value = null;
  await saveFuncCategories();
}

async function deleteFuncCategory(name: string) {
  funcCategories.value = funcCategories.value.filter((c) => c.name !== name);
  await saveFuncCategories();
}

async function saveFileRules() {
  try {
    await invoke("update_rules", { rules: { categories: fileRules.value } });
  } catch (e) {
    console.error(e);
  }
}

async function addFileRule() {
  if (!newRuleName.value.trim()) return;
  fileRules.value.push({
    name: newRuleName.value.trim(),
    target_path: newRuleTarget.value.trim(),
    extensions: newRuleExts.value.split(",").map((s) => s.trim()).filter(Boolean),
    name_keywords: newRuleKeywords.value.split(",").map((s) => s.trim()).filter(Boolean),
  });
  newRuleName.value = "";
  newRuleTarget.value = "";
  newRuleExts.value = "";
  newRuleKeywords.value = "";
  await saveFileRules();
}

async function startEditRule(rule: FileRule, idx: number) {
  editingRuleIdx.value = idx;
  editRuleName.value = rule.name;
  editRuleTarget.value = rule.target_path;
  editRuleExts.value = (rule.extensions || []).join(", ");
  editRuleKeywords.value = (rule.name_keywords || []).join(", ");
}

async function saveFileRule() {
  if (editingRuleIdx.value == null) return;
  const idx = editingRuleIdx.value;
  fileRules.value[idx] = {
    name: editRuleName.value.trim(),
    target_path: editRuleTarget.value.trim(),
    extensions: editRuleExts.value.split(",").map((s) => s.trim()).filter(Boolean),
    name_keywords: editRuleKeywords.value.split(",").map((s) => s.trim()).filter(Boolean),
  };
  editingRuleIdx.value = null;
  await saveFileRules();
}

async function deleteFileRule(name: string) {
  fileRules.value = fileRules.value.filter((r) => r.name !== name);
  await saveFileRules();
}

onMounted(() => {
  fetchFuncCategories();
  fetchFileRules();
});
</script>

<template>
  <div class="p-6 space-y-4">
    <div class="flex items-center gap-2">
      <FolderOpen class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">分类管理</h1>
    </div>

    <Tabs default-value="func">
      <TabsList>
        <TabsTrigger value="func">AI 功能分类</TabsTrigger>
        <TabsTrigger value="file">文件类型分类</TabsTrigger>
      </TabsList>

      <!-- AI 功能分类 -->
      <TabsContent value="func" class="space-y-4">
        <Card>
          <CardContent class="p-4">
            <div class="flex items-center gap-2 mb-3">
              <h3 class="font-medium">新增功能分类</h3>
            </div>
            <div class="flex items-center gap-2">
              <Input v-model="newFuncName" placeholder="分类名称" class="flex-1" />
              <Input v-model="newFuncDesc" placeholder="描述" class="flex-1" />
              <Button @click="addFuncCategory" size="sm">
                <Plus class="h-4 w-4 mr-1" /> 新增
              </Button>
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent class="p-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>名称</TableHead>
                  <TableHead>描述</TableHead>
                  <TableHead class="w-[100px]">操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow v-if="funcCategories.length === 0">
                  <TableCell colspan="3">
                    <Empty message="暂无功能分类" />
                  </TableCell>
                </TableRow>
                <TableRow v-for="(cat, idx) in funcCategories" :key="cat.name">
                  <template v-if="editingFuncName === cat.name">
                    <TableCell><Input v-model="editFuncName" class="h-8" /></TableCell>
                    <TableCell><Input v-model="editFuncDesc" class="h-8" /></TableCell>
                    <TableCell>
                      <div class="flex items-center gap-1">
                        <Button variant="ghost" size="sm" class="h-7" @click="saveFuncCategory">
                          <Save class="h-3.5 w-3.5" />
                        </Button>
                        <Button variant="ghost" size="sm" class="h-7" @click="editingFuncName = null">
                          <X class="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </TableCell>
                  </template>
                  <template v-else>
                    <TableCell class="font-medium">{{ cat.name }}</TableCell>
                    <TableCell class="text-muted-foreground">{{ cat.description || '-' }}</TableCell>
                    <TableCell>
                      <div class="flex items-center gap-1">
                        <Button variant="ghost" size="sm" class="h-7" @click="startEditFunc(cat)">
                          <Pencil class="h-3.5 w-3.5" />
                        </Button>
                        <Button variant="ghost" size="sm" class="h-7" @click="deleteFuncCategory(cat.name)">
                          <Trash2 class="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </TableCell>
                  </template>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </TabsContent>

      <!-- 文件类型分类 -->
      <TabsContent value="file" class="space-y-4">
        <Card>
          <CardContent class="p-4">
            <div class="flex items-center gap-2 mb-3">
              <h3 class="font-medium">新增文件规则</h3>
            </div>
            <div class="grid grid-cols-2 gap-2 mb-2">
              <Input v-model="newRuleName" placeholder="规则名称" />
              <Input v-model="newRuleTarget" placeholder="目标路径" />
            </div>
            <div class="grid grid-cols-2 gap-2 mb-2">
              <Input v-model="newRuleExts" placeholder="扩展名（逗号分隔，如 .exe,.msi）" />
              <Input v-model="newRuleKeywords" placeholder="关键词（逗号分隔）" />
            </div>
            <Button @click="addFileRule" size="sm">
              <Plus class="h-4 w-4 mr-1" /> 新增
            </Button>
          </CardContent>
        </Card>

        <Card>
          <CardContent class="p-4">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>名称</TableHead>
                  <TableHead>目标路径</TableHead>
                  <TableHead>扩展名</TableHead>
                  <TableHead>关键词</TableHead>
                  <TableHead class="w-[100px]">操作</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                <TableRow v-if="fileRules.length === 0">
                  <TableCell colspan="5">
                    <Empty message="暂无文件规则" />
                  </TableCell>
                </TableRow>
                <TableRow v-for="(rule, idx) in fileRules" :key="rule.name">
                  <template v-if="editingRuleIdx === idx">
                    <TableCell><Input v-model="editRuleName" class="h-8" /></TableCell>
                    <TableCell><Input v-model="editRuleTarget" class="h-8" /></TableCell>
                    <TableCell><Input v-model="editRuleExts" class="h-8" /></TableCell>
                    <TableCell><Input v-model="editRuleKeywords" class="h-8" /></TableCell>
                    <TableCell>
                      <div class="flex items-center gap-1">
                        <Button variant="ghost" size="sm" class="h-7" @click="saveFileRule">
                          <Save class="h-3.5 w-3.5" />
                        </Button>
                        <Button variant="ghost" size="sm" class="h-7" @click="editingRuleIdx = null">
                          <X class="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </TableCell>
                  </template>
                  <template v-else>
                    <TableCell class="font-medium">{{ rule.name }}</TableCell>
                    <TableCell class="text-muted-foreground">{{ rule.target_path }}</TableCell>
                    <TableCell>
                      <div class="flex flex-wrap gap-1">
                        <Badge v-for="ext in (rule.extensions || [])" :key="ext" variant="outline" class="text-[10px]">
                          {{ ext }}
                        </Badge>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div class="flex flex-wrap gap-1">
                        <Badge v-for="kw in (rule.name_keywords || [])" :key="kw" variant="outline" class="text-[10px]">
                          {{ kw }}
                        </Badge>
                      </div>
                    </TableCell>
                    <TableCell>
                      <div class="flex items-center gap-1">
                        <Button variant="ghost" size="sm" class="h-7" @click="startEditRule(rule, idx)">
                          <Pencil class="h-3.5 w-3.5" />
                        </Button>
                        <Button variant="ghost" size="sm" class="h-7" @click="deleteFileRule(rule.name)">
                          <Trash2 class="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    </TableCell>
                  </template>
                </TableRow>
              </TableBody>
            </Table>
          </CardContent>
        </Card>
      </TabsContent>
    </Tabs>
  </div>
</template>
