<script setup lang="ts">
import { ref, onMounted, computed } from "vue";
import { pluginInvoke } from "@/lib/pluginInvoke";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Badge } from "@/components/ui/badge";
import { Card } from "@/components/ui/card";
import {
  Table,
  TableHeader,
  TableBody,
  TableRow,
  TableHead,
  TableCell,
} from "@/components/ui/table";
import { Empty } from "@/components/ui/empty";
import { Switch } from "@/components/ui/switch";
import { Checkbox } from "@/components/ui/checkbox";
import { ColumnFilter } from "@/components/ui/column-filter";
import {
  Tabs,
  TabsList,
  TabsTrigger,
  TabsContent,
} from "@/components/ui/tabs";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import {
  Select,
  SelectTrigger,
  SelectContent,
  SelectItem,
  SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
  Settings2,
  Plus,
  Pencil,
  Trash2,
  FolderTree,
  Tags,
  Ban,
  Save,
  X,
  Search,
} from "lucide-vue-next";

// ═════════════ 共用类型 ═════════════

interface SoftwareRoot {
  id: number;
  path: string;
  enabled: boolean;
  display_name: string;
}
interface CategoryRuleRow {
  id: number;
  name: string;
  target_path: string;
  extensions: string[];
  app_dir_only: boolean;
  priority: number;
  enabled: boolean;
}
interface FuncCategoryRow {
  id: number;
  name: string;
  keywords: string[];
  parent: string;
  description: string;
  target_path: string;
  enabled: boolean;
}
interface ExcludeRule {
  id: number;
  rule_type: "dir" | "ext" | "name";
  pattern: string;
  enabled: boolean;
}

interface TagItem {
  id: string;
  name: string;
  color: string;
  description: string;
  count: number;
}

const PLUGIN = "filesweep";
const call = <T>(action: string, args?: Record<string, any>) =>
  pluginInvoke<T>(PLUGIN, action, args);

// ═════════════ 0. 迁移根目录 ═════════════

const migrateRootDir = ref("");
const migrateRootLoading = ref(false);
const enableFuncClassify = ref(false);

async function fetchMigrateRoot() {
  try {
    const settings = await call<any>("settings:get");
    migrateRootDir.value = settings?.migrate_root_dir || "";
    enableFuncClassify.value = settings?.enable_func_classify || false;
  } catch (e) {
    console.error("[settings:get]", e);
  }
}

async function saveMigrateRoot() {
  migrateRootLoading.value = true;
  try {
    await call("settings:update", { migrate_root_dir: migrateRootDir.value.trim() });
  } catch (e) {
    console.error(e);
  } finally {
    migrateRootLoading.value = false;
  }
}

async function saveFuncClassify(v: boolean) {
  enableFuncClassify.value = v;
  try {
    await call("settings:update", { enable_func_classify: v });
  } catch (e) {
    console.error(e);
  }
}

// ═════════════ 1. 软件安装路径 ═════════════

const roots = ref<SoftwareRoot[]>([]);
const rootsLoading = ref(false);

async function fetchRoots() {
  rootsLoading.value = true;
  try {
    roots.value = await call<SoftwareRoot[]>("config:roots:list");
  } catch (e) {
    console.error("[config:roots:list]", e);
    roots.value = [];
  } finally {
    rootsLoading.value = false;
  }
}

async function toggleRoot(r: SoftwareRoot, v: boolean) {
  r.enabled = v;
  try {
    await call("config:roots:update", { id: r.id, enabled: v });
  } catch (e) {
    r.enabled = !v; // 回滚
    console.error(e);
  }
}

const rootDialogOpen = ref(false);
const rootEditing = ref<SoftwareRoot | null>(null);
const rootForm = ref({ path: "", display_name: "" });

function openRootAdd() {
  rootEditing.value = null;
  rootForm.value = { path: "", display_name: "" };
  rootDialogOpen.value = true;
}
function openRootEdit(r: SoftwareRoot) {
  rootEditing.value = r;
  rootForm.value = { path: r.path, display_name: r.display_name };
  rootDialogOpen.value = true;
}

async function saveRoot() {
  if (!rootForm.value.path.trim()) return;
  try {
    if (rootEditing.value) {
      await call("config:roots:update", {
        id: rootEditing.value.id,
        path: rootForm.value.path.trim(),
        display_name: rootForm.value.display_name.trim(),
      });
    } else {
      await call("config:roots:add", {
        path: rootForm.value.path.trim(),
        display_name: rootForm.value.display_name.trim(),
      });
    }
    rootDialogOpen.value = false;
    await fetchRoots();
  } catch (e) {
    console.error(e);
  }
}

async function deleteRoot(id: number) {
  if (!confirm("确定删除此软件根路径？")) return;
  try {
    await call("config:roots:delete", { id });
    await fetchRoots();
  } catch (e) {
    console.error(e);
  }
}

// ═════════════ 2. 分类规则 ═════════════

const rules = ref<CategoryRuleRow[]>([]);
const rulesLoading = ref(false);

async function fetchRules() {
  rulesLoading.value = true;
  try {
    rules.value = await call<CategoryRuleRow[]>("config:categories:list");
  } catch (e) {
    console.error("[config:categories:list]", e);
    rules.value = [];
  } finally {
    rulesLoading.value = false;
  }
}

async function toggleRule(r: CategoryRuleRow, v: boolean) {
  r.enabled = v;
  try {
    await call("config:categories:update", { ...r, enabled: v });
  } catch (e) {
    r.enabled = !v;
    console.error(e);
  }
}

const ruleDialogOpen = ref(false);
const ruleEditing = ref<CategoryRuleRow | null>(null);
const ruleForm = ref({
  name: "",
  target_path: "",
  extensionsText: "",
  app_dir_only: false,
  priority: "0",
});

function openRuleAdd() {
  ruleEditing.value = null;
  ruleForm.value = {
    name: "",
    target_path: "",
    extensionsText: "",
    app_dir_only: false,
    priority: "0",
  };
  ruleDialogOpen.value = true;
}
function openRuleEdit(r: CategoryRuleRow) {
  ruleEditing.value = r;
  ruleForm.value = {
    name: r.name,
    target_path: r.target_path,
    extensionsText: r.extensions.join(", "),
    app_dir_only: r.app_dir_only,
    priority: String(r.priority),
  };
  ruleDialogOpen.value = true;
}

function parseTags(s: string): string[] {
  return s
    .split(",")
    .map((t) => t.trim())
    .filter(Boolean);
}

async function saveRule() {
  if (!ruleForm.value.name.trim()) return;
  const payload = {
    name: ruleForm.value.name.trim(),
    target_path: ruleForm.value.target_path.trim(),
    extensions: parseTags(ruleForm.value.extensionsText),
    app_dir_only: ruleForm.value.app_dir_only,
    priority: parseInt(ruleForm.value.priority, 10) || 0,
    enabled: true,
  };
  try {
    if (ruleEditing.value) {
      await call("config:categories:update", { id: ruleEditing.value.id, ...payload });
    } else {
      await call("config:categories:add", payload);
    }
    ruleDialogOpen.value = false;
    await fetchRules();
  } catch (e) {
    console.error(e);
  }
}

async function deleteRule(id: number) {
  if (!confirm("确定删除此分类规则？")) return;
  try {
    await call("config:categories:delete", { id });
    await fetchRules();
  } catch (e) {
    console.error(e);
  }
}

// ── 分类规则：搜索 / 过滤 / 批量 ──
const rulesSearch = ref("");
const rulesFilterField = ref<"name" | "target_path" | "extensions">("name");
const rulesSelected = ref<Set<number>>(new Set());

// 字段值筛选（ColumnFilter 多选）
const ruleNameFilter = ref<string[]>([]);
const ruleTargetFilter = ref<string[]>([]);
const ruleAppDirFilter = ref<string[]>([]);
const rulePriorityFilter = ref<string[]>([]);

const ruleNameOptions = computed(() => Array.from(new Set(rules.value.map((r) => r.name))).sort());
const ruleTargetOptions = computed(() => Array.from(new Set(rules.value.map((r) => r.target_path).filter(Boolean))).sort());
const ruleAppDirOptions = computed(() => ["是", "否"]);
const rulePriorityOptions = computed(() => Array.from(new Set(rules.value.map((r) => String(r.priority)))).sort());

const filteredRules = computed(() => {
  const q = rulesSearch.value.trim().toLowerCase();
  return rules.value.filter((r) => {
    // 搜索过滤
    if (q) {
      const field =
        rulesFilterField.value === "name" ? r.name
        : rulesFilterField.value === "target_path" ? r.target_path
        : r.extensions.join(" ");
      if (!field.toLowerCase().includes(q)) return false;
    }
    // 字段值筛选
    if (ruleNameFilter.value.length > 0 && !ruleNameFilter.value.includes(r.name)) return false;
    if (ruleTargetFilter.value.length > 0 && !ruleTargetFilter.value.includes(r.target_path)) return false;
    if (ruleAppDirFilter.value.length > 0) {
      const label = r.app_dir_only ? "是" : "否";
      if (!ruleAppDirFilter.value.includes(label)) return false;
    }
    if (rulePriorityFilter.value.length > 0 && !rulePriorityFilter.value.includes(String(r.priority))) return false;
    return true;
  });
});

function toggleRuleSelect(id: number) {
  const s = new Set(rulesSelected.value);
  s.has(id) ? s.delete(id) : s.add(id);
  rulesSelected.value = s;
}
function toggleRuleSelectAll() {
  rulesSelected.value =
    rulesSelected.value.size === filteredRules.value.length
      ? new Set()
      : new Set(filteredRules.value.map((r) => r.id));
}

async function batchRuleToggle(enabled: boolean) {
  const ids = Array.from(rulesSelected.value);
  for (const id of ids) {
    const r = rules.value.find((x) => x.id === id);
    if (!r) continue;
    try {
      await call("config:categories:update", { ...r, enabled });
      r.enabled = enabled;
    } catch (e) {
      console.error(e);
    }
  }
  rulesSelected.value = new Set();
}
async function batchRuleDelete() {
  if (!confirm(`确定删除选中的 ${rulesSelected.value.size} 条规则？`)) return;
  for (const id of Array.from(rulesSelected.value)) {
    try {
      await call("config:categories:delete", { id });
    } catch (e) {
      console.error(e);
    }
  }
  rulesSelected.value = new Set();
  await fetchRules();
}

// ═════════════ 3. 功能分类 ═════════════

const funcCats = ref<FuncCategoryRow[]>([]);
const funcLoading = ref(false);

async function fetchFuncCats() {
  funcLoading.value = true;
  try {
    funcCats.value = await call<FuncCategoryRow[]>("config:func_categories:list");
  } catch (e) {
    console.error("[config:func_categories:list]", e);
    funcCats.value = [];
  } finally {
    funcLoading.value = false;
  }
}

async function toggleFunc(c: FuncCategoryRow, v: boolean) {
  c.enabled = v;
  try {
    await call("config:func_categories:update", { ...c, enabled: v });
  } catch (e) {
    c.enabled = !v;
    console.error(e);
  }
}

const funcDialogOpen = ref(false);
const funcEditing = ref<FuncCategoryRow | null>(null);
const funcForm = ref({ name: "", parent: "", keywordsText: "", description: "", targetPath: "" });

function openFuncAdd() {
  funcEditing.value = null;
  funcForm.value = { name: "", parent: "", keywordsText: "", description: "", targetPath: "" };
  funcDialogOpen.value = true;
}
function openFuncEdit(c: FuncCategoryRow) {
  funcEditing.value = c;
  funcForm.value = {
    name: c.name,
    parent: c.parent,
    keywordsText: c.keywords.join(", "),
    description: c.description,
    targetPath: c.target_path,
  };
  funcDialogOpen.value = true;
}

async function saveFunc() {
  if (!funcForm.value.name.trim()) return;
  const payload = {
    name: funcForm.value.name.trim(),
    parent: funcForm.value.parent.trim(),
    keywords: parseTags(funcForm.value.keywordsText),
    description: funcForm.value.description.trim(),
    target_path: funcForm.value.targetPath.trim(),
    enabled: true,
  };
  try {
    if (funcEditing.value) {
      await call("config:func_categories:update", { id: funcEditing.value.id, ...payload });
    } else {
      await call("config:func_categories:add", payload);
    }
    funcDialogOpen.value = false;
    await fetchFuncCats();
  } catch (e) {
    console.error(e);
  }
}

async function deleteFunc(id: number) {
  if (!confirm("确定删除此功能分类？")) return;
  try {
    await call("config:func_categories:delete", { id });
    await fetchFuncCats();
  } catch (e) {
    console.error(e);
  }
}

// ── 功能分类：搜索 / 过滤 / 批量 ──
const funcSearch = ref("");
const funcFilterField = ref<"name" | "parent" | "keywords" | "description">("name");
const funcSelected = ref<Set<number>>(new Set());
const funcParentFilter = ref<string[]>([]);

// 父分类所有唯一值（去重排序）
const funcParentOptions = computed(() => {
  const set = new Set(funcCats.value.map((c) => c.parent));
  return Array.from(set).sort();
});

const filteredFuncCats = computed(() => {
  const q = funcSearch.value.trim().toLowerCase();
  return funcCats.value.filter((c) => {
    // 搜索过滤
    if (q) {
      const field =
        funcFilterField.value === "name" ? c.name
        : funcFilterField.value === "parent" ? c.parent
        : funcFilterField.value === "keywords" ? c.keywords.join(" ")
        : c.description;
      if (!field.toLowerCase().includes(q)) return false;
    }
    // 父分类值筛选
    if (funcParentFilter.value.length > 0 && !funcParentFilter.value.includes(c.parent)) {
      return false;
    }
    return true;
  });
});

function toggleFuncSelect(id: number) {
  const s = new Set(funcSelected.value);
  s.has(id) ? s.delete(id) : s.add(id);
  funcSelected.value = s;
}
function toggleFuncSelectAll() {
  funcSelected.value =
    funcSelected.value.size === filteredFuncCats.value.length
      ? new Set()
      : new Set(filteredFuncCats.value.map((c) => c.id));
}

async function batchFuncToggle(enabled: boolean) {
  const ids = Array.from(funcSelected.value);
  for (const id of ids) {
    const c = funcCats.value.find((x) => x.id === id);
    if (!c) continue;
    try {
      await call("config:func_categories:update", { ...c, enabled });
      c.enabled = enabled;
    } catch (e) {
      console.error(e);
    }
  }
  funcSelected.value = new Set();
}
async function batchFuncDelete() {
  if (!confirm(`确定删除选中的 ${funcSelected.value.size} 条功能分类？`)) return;
  for (const id of Array.from(funcSelected.value)) {
    try {
      await call("config:func_categories:delete", { id });
    } catch (e) {
      console.error(e);
    }
  }
  funcSelected.value = new Set();
  await fetchFuncCats();
}

// ═════════════ 4. 排除规则 ═════════════

const excludes = ref<ExcludeRule[]>([]);
const excludesLoading = ref(false);

async function fetchExcludes() {
  excludesLoading.value = true;
  try {
    excludes.value = await call<ExcludeRule[]>("config:exclude:list");
  } catch (e) {
    console.error("[config:exclude:list]", e);
    excludes.value = [];
  } finally {
    excludesLoading.value = false;
  }
}

async function toggleExclude(r: ExcludeRule, v: boolean) {
  r.enabled = v;
  try {
    await call("config:exclude:update", { id: r.id, enabled: v });
  } catch (e) {
    r.enabled = !v;
    console.error(e);
  }
}

const excludeForm = ref<{ rule_type: "dir" | "ext" | "name"; pattern: string }>({
  rule_type: "dir",
  pattern: "",
});

async function addExclude() {
  if (!excludeForm.value.pattern.trim()) return;
  try {
    await call("config:exclude:add", {
      rule_type: excludeForm.value.rule_type,
      pattern: excludeForm.value.pattern.trim(),
    });
    excludeForm.value.pattern = "";
    await fetchExcludes();
  } catch (e) {
    console.error(e);
  }
}

async function deleteExclude(id: number) {
  try {
    await call("config:exclude:delete", { id });
    await fetchExcludes();
  } catch (e) {
    console.error(e);
  }
}

const excludeTypeLabel: Record<string, string> = {
  dir: "目录",
  ext: "扩展名",
  name: "文件名",
};

// ═════════════ 5. 标签 ═════════════

const tags = ref<TagItem[]>([]);
const tagsLoading = ref(false);
const editingTagId = ref<string | null>(null);

const newTagName = ref("");
const newTagColor = ref("#3b82f6");
const newTagDesc = ref("");

const editTagName = ref("");
const editTagColor = ref("");
const editTagDesc = ref("");

const presetColors = [
  "#3b82f6", "#ef4444", "#22c55e", "#f59e0b", "#8b5cf6",
  "#ec4899", "#06b6d4", "#f97316", "#14b8a6", "#6366f1",
  "#84cc16", "#e11d48",
];

async function fetchTags() {
  tagsLoading.value = true;
  try {
    tags.value = await call<TagItem[]>("config:tags:list");
  } catch (e) {
    console.error("[config:tags:list]", e);
    tags.value = [];
  } finally {
    tagsLoading.value = false;
  }
}

async function addTag() {
  if (!newTagName.value.trim()) return;
  try {
    await call("config:tags:add", {
      name: newTagName.value.trim(),
      color: newTagColor.value,
      description: newTagDesc.value.trim(),
    });
    newTagName.value = "";
    newTagDesc.value = "";
    await fetchTags();
  } catch (e) {
    console.error(e);
  }
}

function startEditTag(tag: TagItem) {
  editingTagId.value = tag.id;
  editTagName.value = tag.name;
  editTagColor.value = tag.color;
  editTagDesc.value = tag.description;
}

async function saveEditTag() {
  if (!editingTagId.value) return;
  try {
    await call("config:tags:update", {
      id: editingTagId.value,
      name: editTagName.value.trim(),
      color: editTagColor.value,
      description: editTagDesc.value.trim(),
    });
    editingTagId.value = null;
    await fetchTags();
  } catch (e) {
    console.error(e);
  }
}

async function deleteTag(id: string) {
  if (!confirm("确定删除此标签？")) return;
  try {
    await call("config:tags:delete", { id });
    await fetchTags();
  } catch (e) {
    console.error(e);
  }
}

// ═════════════ 初始化 ═════════════

onMounted(() => {
  fetchMigrateRoot();
  fetchRoots();
  fetchRules();
  fetchFuncCats();
  fetchExcludes();
  fetchTags();
});
</script>

<template>
  <div class="p-6 space-y-4">
    <div class="flex items-center gap-2">
      <Settings2 class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">规则管理</h1>
      <span class="text-sm text-muted-foreground">扫描规则、根路径、排除项、标签（DB 持久化）</span>
    </div>

    <!-- 迁移根目录 + 功能分类开关（跨 tab 共享） -->
    <Card class="p-4">
      <div class="flex items-end gap-6">
        <div class="space-y-1 flex-1">
          <Label class="text-xs">迁移根目录</Label>
          <Input
            v-model="migrateRootDir"
            placeholder="如 D:\Archive 或留空（功能分类/分类规则的目标路径为相对路径时拼接此目录）"
            @blur="saveMigrateRoot"
          />
        </div>
        <Button size="sm" :disabled="migrateRootLoading" @click="saveMigrateRoot">
          <Save class="h-4 w-4 mr-1" /> 保存
        </Button>
        <div class="flex items-center gap-2 pb-1">
          <Switch v-model="enableFuncClassify" @update:model-value="saveFuncClassify" />
          <Label class="text-xs cursor-pointer" @click="enableFuncClassify = !enableFuncClassify; saveFuncClassify(enableFuncClassify)">
            功能分类匹配
          </Label>
        </div>
      </div>
      <p class="text-xs text-muted-foreground mt-2">
        target_path 填绝对路径则直接使用；填相对路径则拼接迁移根目录。功能分类开启后扫描时用 func_categories 关键词匹配文件归属行业
      </p>
    </Card>

    <Tabs default-value="roots" class="w-full">
      <TabsList>
        <TabsTrigger value="roots">软件安装路径</TabsTrigger>
        <TabsTrigger value="rules">分类规则</TabsTrigger>
        <TabsTrigger value="func">功能分类</TabsTrigger>
        <TabsTrigger value="exclude">排除规则</TabsTrigger>
        <TabsTrigger value="tags">标签</TabsTrigger>
      </TabsList>

      <!-- ═════════ 软件安装路径 ═════════ -->
      <TabsContent value="roots" class="space-y-3">
        <div class="flex items-center justify-between">
          <p class="text-sm text-muted-foreground">
            扫描时一级子目录直接识别为 app dir（秒级，不递归）
          </p>
          <Button size="sm" @click="openRootAdd">
            <Plus class="h-4 w-4 mr-1" /> 新增路径
          </Button>
        </div>
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="w-[60px]">启用</TableHead>
                <TableHead>路径</TableHead>
                <TableHead class="w-[180px]">显示名</TableHead>
                <TableHead class="w-[100px] text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-if="roots.length === 0">
                <TableCell :colspan="4" class="h-32">
                  <Empty :icon="FolderTree" message="暂无软件根路径" />
                </TableCell>
              </TableRow>
              <TableRow v-for="r in roots" :key="r.id">
                <TableCell>
                  <Switch :model-value="r.enabled" @update:model-value="(v) => toggleRoot(r, v)" />
                </TableCell>
                <TableCell class="font-mono text-xs">{{ r.path }}</TableCell>
                <TableCell class="text-sm text-muted-foreground">
                  {{ r.display_name || "-" }}
                </TableCell>
                <TableCell class="text-right">
                  <div class="flex items-center justify-end gap-1">
                    <Button variant="ghost" size="icon" class="h-7 w-7" @click="openRootEdit(r)">
                      <Pencil class="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      class="h-7 w-7 text-destructive"
                      @click="deleteRoot(r.id)"
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Card>
      </TabsContent>

      <!-- ═════════ 分类规则 ═════════ -->
      <TabsContent value="rules" class="space-y-3">
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2 flex-1">
            <Select v-model="rulesFilterField">
              <SelectTrigger class="w-[140px] h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="name">名称</SelectItem>
                <SelectItem value="target_path">目标路径</SelectItem>
                <SelectItem value="extensions">扩展名</SelectItem>
              </SelectContent>
            </Select>
            <div class="relative flex-1 max-w-[300px]">
              <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input v-model="rulesSearch" placeholder="搜索..." class="pl-9 h-8" />
            </div>
          </div>
          <Button size="sm" @click="openRuleAdd">
            <Plus class="h-4 w-4 mr-1" /> 新增规则
          </Button>
        </div>

        <!-- 批量操作栏 -->
        <div v-if="rulesSelected.size > 0" class="flex items-center gap-3 p-3 bg-muted rounded-lg">
          <span class="text-sm">已选择 {{ rulesSelected.size }} 个</span>
          <Button size="sm" variant="outline" @click="batchRuleToggle(true)">批量启用</Button>
          <Button size="sm" variant="outline" @click="batchRuleToggle(false)">批量禁用</Button>
          <Button size="sm" variant="destructive" @click="batchRuleDelete">批量删除</Button>
          <Button size="sm" variant="ghost" @click="rulesSelected = new Set()">取消</Button>
        </div>

        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="w-[40px]">
                  <Checkbox
                    :model-value="rulesSelected.size === filteredRules.length && filteredRules.length > 0"
                    @update:model-value="toggleRuleSelectAll"
                  />
                </TableHead>
                <TableHead class="w-[50px]">启用</TableHead>
                <TableHead>
                  <div class="inline-flex items-center gap-1">
                    名称
                    <ColumnFilter :options="ruleNameOptions" v-model="ruleNameFilter" />
                  </div>
                </TableHead>
                <TableHead>
                  <div class="inline-flex items-center gap-1">
                    目标路径
                    <ColumnFilter :options="ruleTargetOptions" v-model="ruleTargetFilter" />
                  </div>
                </TableHead>
                <TableHead>扩展名</TableHead>
                <TableHead class="w-[70px]">
                  <div class="inline-flex items-center gap-1">
                    AppDir
                    <ColumnFilter :options="ruleAppDirOptions" v-model="ruleAppDirFilter" />
                  </div>
                </TableHead>
                <TableHead class="w-[60px]">
                  <div class="inline-flex items-center gap-1">
                    优先级
                    <ColumnFilter :options="rulePriorityOptions" v-model="rulePriorityFilter" />
                  </div>
                </TableHead>
                <TableHead class="w-[80px] text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-if="filteredRules.length === 0">
                <TableCell :colspan="8" class="h-32">
                  <Empty :icon="FolderTree" message="暂无分类规则" />
                </TableCell>
              </TableRow>
              <TableRow v-for="r in filteredRules" :key="r.id">
                <TableCell>
                  <Checkbox
                    :model-value="rulesSelected.has(r.id)"
                    @update:model-value="() => toggleRuleSelect(r.id)"
                  />
                </TableCell>
                <TableCell>
                  <Switch :model-value="r.enabled" @update:model-value="(v) => toggleRule(r, v)" />
                </TableCell>
                <TableCell class="font-medium">{{ r.name }}</TableCell>
                <TableCell class="font-mono text-xs text-muted-foreground">
                  {{ r.target_path || "-" }}
                </TableCell>
                <TableCell>
                  <div class="flex flex-wrap gap-1 max-w-[200px]">
                    <Badge
                      v-for="e in r.extensions"
                      :key="e"
                      variant="secondary"
                      class="text-[10px]"
                    >{{ e }}</Badge>
                    <span v-if="r.extensions.length === 0" class="text-xs text-muted-foreground">-</span>
                  </div>
                </TableCell>
                <TableCell>
                  <Badge v-if="r.app_dir_only" variant="default" class="text-[10px]">是</Badge>
                  <span v-else class="text-xs text-muted-foreground">-</span>
                </TableCell>
                <TableCell class="text-sm">{{ r.priority }}</TableCell>
                <TableCell class="text-right">
                  <div class="flex items-center justify-end gap-1">
                    <Button variant="ghost" size="icon" class="h-7 w-7" @click="openRuleEdit(r)">
                      <Pencil class="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      class="h-7 w-7 text-destructive"
                      @click="deleteRule(r.id)"
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Card>
      </TabsContent>

      <!-- ═════════ 功能分类 ═════════ -->
      <TabsContent value="func" class="space-y-3">
        <div class="flex items-center justify-between gap-3">
          <div class="flex items-center gap-2 flex-1">
            <Select v-model="funcFilterField">
              <SelectTrigger class="w-[140px] h-8 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="name">名称</SelectItem>
                <SelectItem value="parent">父分类</SelectItem>
                <SelectItem value="keywords">关键词</SelectItem>
                <SelectItem value="description">描述</SelectItem>
              </SelectContent>
            </Select>
            <div class="relative flex-1 max-w-[300px]">
              <Search class="absolute left-2.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground" />
              <Input v-model="funcSearch" placeholder="搜索..." class="pl-9 h-8" />
            </div>
          </div>
          <Button size="sm" @click="openFuncAdd">
            <Plus class="h-4 w-4 mr-1" /> 新增分类
          </Button>
        </div>

        <!-- 批量操作栏 -->
        <div v-if="funcSelected.size > 0" class="flex items-center gap-3 p-3 bg-muted rounded-lg">
          <span class="text-sm">已选择 {{ funcSelected.size }} 个</span>
          <Button size="sm" variant="outline" @click="batchFuncToggle(true)">批量启用</Button>
          <Button size="sm" variant="outline" @click="batchFuncToggle(false)">批量禁用</Button>
          <Button size="sm" variant="destructive" @click="batchFuncDelete">批量删除</Button>
          <Button size="sm" variant="ghost" @click="funcSelected = new Set()">取消</Button>
        </div>

        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="w-[40px]">
                  <Checkbox
                    :model-value="funcSelected.size === filteredFuncCats.length && filteredFuncCats.length > 0"
                    @update:model-value="toggleFuncSelectAll"
                  />
                </TableHead>
                <TableHead class="w-[50px]">启用</TableHead>
                <TableHead>名称</TableHead>
                <TableHead class="w-[120px]">
                  <div class="inline-flex items-center gap-1">
                    父分类
                    <ColumnFilter
                      :options="funcParentOptions"
                      v-model="funcParentFilter"
                    />
                  </div>
                </TableHead>
                <TableHead>关键词</TableHead>
                <TableHead>描述</TableHead>
                <TableHead>目标路径</TableHead>
                <TableHead class="w-[80px] text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-if="filteredFuncCats.length === 0">
                <TableCell :colspan="8" class="h-32">
                  <Empty :icon="Tags" message="暂无功能分类" />
                </TableCell>
              </TableRow>
              <TableRow v-for="c in filteredFuncCats" :key="c.id">
                <TableCell>
                  <Checkbox
                    :model-value="funcSelected.has(c.id)"
                    @update:model-value="() => toggleFuncSelect(c.id)"
                  />
                </TableCell>
                <TableCell>
                  <Switch :model-value="c.enabled" @update:model-value="(v) => toggleFunc(c, v)" />
                </TableCell>
                <TableCell class="font-medium">{{ c.name }}</TableCell>
                <TableCell class="text-sm text-muted-foreground">{{ c.parent || "-" }}</TableCell>
                <TableCell>
                  <div class="flex flex-wrap gap-1 max-w-[180px]">
                    <Badge
                      v-for="k in c.keywords"
                      :key="k"
                      variant="outline"
                      class="text-[10px]"
                    >{{ k }}</Badge>
                    <span v-if="c.keywords.length === 0" class="text-xs text-muted-foreground">-</span>
                  </div>
                </TableCell>
                <TableCell class="text-xs text-muted-foreground max-w-[200px] truncate" :title="c.description">
                  {{ c.description || "-" }}
                </TableCell>
                <TableCell class="font-mono text-xs text-muted-foreground max-w-[160px] truncate" :title="c.target_path">
                  {{ c.target_path || "-" }}
                </TableCell>
                <TableCell class="text-right">
                  <div class="flex items-center justify-end gap-1">
                    <Button variant="ghost" size="icon" class="h-7 w-7" @click="openFuncEdit(c)">
                      <Pencil class="h-3.5 w-3.5" />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      class="h-7 w-7 text-destructive"
                      @click="deleteFunc(c.id)"
                    >
                      <Trash2 class="h-3.5 w-3.5" />
                    </Button>
                  </div>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Card>
      </TabsContent>

      <!-- ═════════ 排除规则 ═════════ -->
      <TabsContent value="exclude" class="space-y-3">
        <p class="text-sm text-muted-foreground">
          扫描时按类型排除（dir=目录名，ext=扩展名，name=文件名）
        </p>
        <Card class="p-4">
          <div class="flex items-end gap-3">
            <div class="space-y-1 w-[140px]">
              <Label class="text-xs">类型</Label>
              <Select v-model="excludeForm.rule_type">
                <SelectTrigger>
                  <SelectValue placeholder="选择类型" />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="dir">目录 (dir)</SelectItem>
                  <SelectItem value="ext">扩展名 (ext)</SelectItem>
                  <SelectItem value="name">文件名 (name)</SelectItem>
                </SelectContent>
              </Select>
            </div>
            <div class="space-y-1 flex-1">
              <Label class="text-xs">匹配模式</Label>
              <Input
                v-model="excludeForm.pattern"
                placeholder="如 Windows / .tmp / Thumbs.db"
                @keyup.enter="addExclude"
              />
            </div>
            <Button @click="addExclude">
              <Plus class="h-4 w-4 mr-1" /> 添加
            </Button>
          </div>
        </Card>
        <Card>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead class="w-[50px]">启用</TableHead>
                <TableHead class="w-[120px]">类型</TableHead>
                <TableHead>模式</TableHead>
                <TableHead class="w-[80px] text-right">操作</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              <TableRow v-if="excludes.length === 0">
                <TableCell :colspan="4" class="h-32">
                  <Empty :icon="Ban" message="暂无排除规则" />
                </TableCell>
              </TableRow>
              <TableRow v-for="r in excludes" :key="r.id">
                <TableCell>
                  <Switch
                    :model-value="r.enabled"
                    @update:model-value="(v) => toggleExclude(r, v)"
                  />
                </TableCell>
                <TableCell>
                  <Badge variant="secondary" class="text-[10px]">
                    {{ excludeTypeLabel[r.rule_type] || r.rule_type }}
                  </Badge>
                </TableCell>
                <TableCell class="font-mono text-xs">{{ r.pattern }}</TableCell>
                <TableCell class="text-right">
                  <Button
                    variant="ghost"
                    size="icon"
                    class="h-7 w-7 text-destructive"
                    @click="deleteExclude(r.id)"
                  >
                    <Trash2 class="h-3.5 w-3.5" />
                  </Button>
                </TableCell>
              </TableRow>
            </TableBody>
          </Table>
        </Card>
      </TabsContent>

      <!-- ═════════ 标签 ═════════ -->
      <TabsContent value="tags" class="space-y-3">
        <p class="text-sm text-muted-foreground">
          软件目录可关联的标签（按颜色分组，AI 丰富时自动归类）
        </p>

        <!-- 新增标签 -->
        <Card class="p-4">
          <div class="flex items-end gap-3">
            <div class="space-y-1">
              <Label class="text-xs">标签名称</Label>
              <Input v-model="newTagName" placeholder="输入标签名称" class="w-[180px]" />
            </div>
            <div class="space-y-1">
              <Label class="text-xs">颜色</Label>
              <div class="flex items-center gap-2">
                <input type="color" v-model="newTagColor" class="w-8 h-8 rounded cursor-pointer border" />
                <div class="flex gap-1">
                  <button
                    v-for="color in presetColors"
                    :key="color"
                    :class="[
                      'w-5 h-5 rounded-full border-2 transition-transform',
                      newTagColor === color ? 'border-foreground scale-110' : 'border-transparent hover:scale-110',
                    ]"
                    :style="{ backgroundColor: color }"
                    @click="newTagColor = color"
                  />
                </div>
              </div>
            </div>
            <div class="space-y-1 flex-1">
              <Label class="text-xs">描述</Label>
              <Input v-model="newTagDesc" placeholder="标签描述（可选）" />
            </div>
            <Button @click="addTag">
              <Plus class="h-4 w-4 mr-1" /> 添加标签
            </Button>
          </div>
        </Card>

        <!-- 标签表格 -->
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
                  <Empty :icon="Tags" message="暂无标签" />
                </TableCell>
              </TableRow>
              <TableRow v-for="tag in tags" :key="tag.id">
                <TableCell>
                  <div
                    v-if="editingTagId === tag.id"
                    class="w-6 h-6 rounded-full border cursor-pointer relative overflow-hidden"
                    :style="{ backgroundColor: editTagColor }"
                  >
                    <input type="color" v-model="editTagColor" class="absolute inset-0 w-full h-full opacity-0 cursor-pointer" />
                  </div>
                  <div v-else class="w-4 h-4 rounded-full" :style="{ backgroundColor: tag.color }" />
                </TableCell>
                <TableCell>
                  <template v-if="editingTagId === tag.id">
                    <Input v-model="editTagName" class="h-8 w-[150px]" />
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
                  <template v-if="editingTagId === tag.id">
                    <Input v-model="editTagDesc" class="h-8" />
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
                    <template v-if="editingTagId === tag.id">
                      <Button variant="outline" size="sm" class="h-7" @click="saveEditTag">
                        <Save class="h-3 w-3 mr-1" /> 保存
                      </Button>
                      <Button variant="ghost" size="sm" class="h-7" @click="editingTagId = null">
                        <X class="h-3 w-3" />
                      </Button>
                    </template>
                    <template v-else>
                      <Button variant="ghost" size="icon" class="h-7 w-7" @click="startEditTag(tag)">
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
      </TabsContent>
    </Tabs>

    <!-- ═════════ 软件根路径 编辑弹窗 ═════════ -->
    <Dialog v-model:open="rootDialogOpen">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ rootEditing ? "编辑软件根路径" : "新增软件根路径" }}</DialogTitle>
          <DialogDescription>扫描时只读取一级子目录</DialogDescription>
        </DialogHeader>
        <div class="space-y-3">
          <div class="space-y-1">
            <Label>路径</Label>
            <Input v-model="rootForm.path" placeholder="如 D:\Program Files" />
          </div>
          <div class="space-y-1">
            <Label>显示名（可选）</Label>
            <Input v-model="rootForm.display_name" placeholder="如 D盘 Program Files" />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="rootDialogOpen = false">取消</Button>
          <Button @click="saveRoot">
            <Save class="h-4 w-4 mr-1" /> 保存
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- ═════════ 分类规则 编辑弹窗 ═════════ -->
    <Dialog v-model:open="ruleDialogOpen">
      <DialogContent class="max-w-lg">
        <DialogHeader>
          <DialogTitle>{{ ruleEditing ? "编辑分类规则" : "新增分类规则" }}</DialogTitle>
          <DialogDescription>按扩展名或关键词匹配文件归类</DialogDescription>
        </DialogHeader>
        <div class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1">
              <Label>规则名</Label>
              <Input v-model="ruleForm.name" placeholder="如 开发工具" />
            </div>
            <div class="space-y-1">
              <Label>目标路径</Label>
              <Input v-model="ruleForm.target_path" placeholder="如 DevTools" />
            </div>
          </div>
          <div class="space-y-1">
            <Label>扩展名（逗号分隔）</Label>
            <Input v-model="ruleForm.extensionsText" placeholder=".exe, .msi, .dll" />
          </div>
          <div class="grid grid-cols-2 gap-3">
            <div class="flex items-center gap-2">
              <Switch v-model="ruleForm.app_dir_only" />
              <Label class="cursor-pointer" @click="ruleForm.app_dir_only = !ruleForm.app_dir_only">
                仅 App Dir
              </Label>
            </div>
            <div class="space-y-1">
              <Label>优先级（高优先匹配）</Label>
              <Input v-model="ruleForm.priority" type="number" placeholder="0" />
            </div>
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="ruleDialogOpen = false">取消</Button>
          <Button @click="saveRule">
            <Save class="h-4 w-4 mr-1" /> 保存
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>

    <!-- ═════════ 功能分类 编辑弹窗 ═════════ -->
    <Dialog v-model:open="funcDialogOpen">
      <DialogContent class="max-w-md">
        <DialogHeader>
          <DialogTitle>{{ funcEditing ? "编辑功能分类" : "新增功能分类" }}</DialogTitle>
          <DialogDescription>功能性归类（按关键词识别）</DialogDescription>
        </DialogHeader>
        <div class="space-y-3">
          <div class="grid grid-cols-2 gap-3">
            <div class="space-y-1">
              <Label>名称</Label>
              <Input v-model="funcForm.name" placeholder="如 引导管理" />
            </div>
            <div class="space-y-1">
              <Label>父分类</Label>
              <Input v-model="funcForm.parent" placeholder="如 操作系统" />
            </div>
          </div>
          <div class="space-y-1">
            <Label>关键词（逗号分隔）</Label>
            <Input v-model="funcForm.keywordsText" placeholder="EasyBCD, rEFInd" />
          </div>
          <div class="space-y-1">
            <Label>描述</Label>
            <Textarea v-model="funcForm.description" :rows="2" placeholder="分类描述（可选）" />
          </div>
          <div class="space-y-1">
            <Label>目标路径（\ 分隔，最多 4 级）</Label>
            <Input v-model="funcForm.targetPath" placeholder="如 Security\Exploit\Frameworks" />
          </div>
        </div>
        <DialogFooter>
          <Button variant="outline" @click="funcDialogOpen = false">取消</Button>
          <Button @click="saveFunc">
            <Save class="h-4 w-4 mr-1" /> 保存
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
