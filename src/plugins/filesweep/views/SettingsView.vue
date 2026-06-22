<script setup lang="ts">
import { ref, onMounted, computed, watch } from "vue";
import { invoke } from "@/lib/api";
import { useSettingsStore } from "@plugins/filesweep/stores/settings";
import { useFilesStore } from "@plugins/filesweep/stores/files";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from "@/components/ui/card";
import { Separator } from "@/components/ui/separator";
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from "@/components/ui/select";
import {
  Dialog, DialogContent, DialogHeader, DialogFooter, DialogTitle, DialogDescription,
} from "@/components/ui/dialog";
import { Settings, RotateCcw, AlertTriangle, Sparkles, Shield, Zap } from "lucide-vue-next";

const store = useSettingsStore();
const resetDialogOpen = ref(false);

const ruleLabels: Record<keyof typeof store.config.rules, string> = {
  auto_categorize: "自动分类",
  auto_duplicate: "自动去重",
  keep_newest_version: "保留最新版本",
  move_to_recycle_bin: "移至回收站",
  delete_empty_dirs: "删除空目录",
};

const retentionOptions = [
  { value: 7, label: "7 天" },
  { value: 30, label: "30 天" },
  { value: 90, label: "90 天" },
  { value: 180, label: "180 天" },
  { value: 365, label: "365 天" },
];

const providerOptions = [
  { value: "offline", label: "离线规则" },
  { value: "ollama", label: "Ollama" },
  { value: "openai", label: "OpenAI" },
  { value: "claude", label: "Claude" },
  { value: "custom", label: "自定义" },
];

const isOllama = computed(() => store.config.ai.provider === "ollama");
const isOpenai = computed(() => store.config.ai.provider === "openai");
const isClaude = computed(() => store.config.ai.provider === "claude");
const isCustom = computed(() => store.config.ai.provider === "custom");

const aiSaving = ref(false);
const aiSaved = ref(false);

async function toggleRule(key: string) {
  await store.toggleRule(key as keyof typeof store.config.rules);
}

// AI 配置改为本地修改 + 保存按钮
async function updateAiProvider(value: string) {
  store.config.ai.provider = value;
}

async function saveAiSettings() {
  aiSaving.value = true;
  try {
    await store.updateSettings({ ai: { ...store.config.ai } });
    aiSaved.value = true;
    setTimeout(() => { aiSaved.value = false; }, 1500);
  } catch (e) {
    console.error(e);
  } finally {
    aiSaving.value = false;
  }
}

async function updatePrivacyField(field: string, value: any) {
  await store.updateSettings({
    privacy: { ...store.config.privacy, [field]: value },
  });
}

async function resetDefaults() {
  await store.resetDefaults();
}

async function resetDatabase() {
  try {
    await store.resetDatabase();
    // 清空文件列表前端状态（DB 已清空，前端也要同步）
    const filesStore = useFilesStore();
    filesStore.files = [];
    filesStore.total = 0;
    resetDialogOpen.value = false;
  } catch (e) {
    console.error(e);
  }
}

onMounted(() => {
  store.fetchSettings();
});
</script>

<template>
  <div class="p-6 space-y-6 max-w-2xl">
    <div class="flex items-center gap-2">
      <Settings class="h-5 w-5 text-primary" />
      <h1 class="text-xl font-bold">设置</h1>
    </div>

    <!-- Automation Rules -->
    <Card>
      <CardHeader>
        <div class="flex items-center justify-between">
          <div class="flex items-center gap-2">
            <Zap class="h-4 w-4 text-primary" />
            <CardTitle class="text-base">自动化规则</CardTitle>
          </div>
          <Button variant="outline" size="sm" @click="resetDefaults">
            <RotateCcw class="h-3 w-3 mr-1" /> 恢复默认
          </Button>
        </div>
        <CardDescription>配置文件扫描和清理的自动化行为</CardDescription>
      </CardHeader>
      <CardContent class="space-y-4">
        <div
          v-for="(label, key) in ruleLabels"
          :key="key"
          class="flex items-center justify-between"
        >
          <Label>{{ label }}</Label>
          <Switch
            :model-value="store.config.rules[key] as boolean"
            @update:model-value="() => toggleRule(key)"
          />
        </div>
      </CardContent>
    </Card>

    <!-- AI Provider -->
    <Card>
      <CardHeader>
        <div class="flex items-center gap-2">
          <Sparkles class="h-4 w-4 text-primary" />
          <CardTitle class="text-base">AI 提供商</CardTitle>
        </div>
        <CardDescription>配置 AI 智能分类和元数据丰富</CardDescription>
      </CardHeader>
      <CardContent class="space-y-4">
        <div class="space-y-1">
          <Label>提供商</Label>
          <Select
            :model-value="store.config.ai.provider"
            @update:model-value="(v: string) => updateAiProvider(v)"
          >
            <SelectTrigger class="w-full">
              <SelectValue placeholder="选择 AI 提供商" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="opt in providerOptions" :key="opt.value" :value="opt.value">
                {{ opt.label }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>

        <!-- Ollama Config -->
        <template v-if="isOllama">
          <div class="space-y-1">
            <Label>服务地址</Label>
            <Input
              :model-value="store.config.ai.ollama_url || ''"
              placeholder="http://localhost:11434"
              @update:model-value="v => store.config.ai.ollama_url = v"
            />
          </div>
          <div class="space-y-1">
            <Label>模型名称</Label>
            <Input
              :model-value="store.config.ai.ollama_model || ''"
              placeholder="llama3, qwen2..."
              @update:model-value="v => store.config.ai.ollama_model = v"
            />
          </div>
        </template>

        <!-- OpenAI Config -->
        <template v-if="isOpenai">
          <div class="space-y-1">
            <Label>API Key</Label>
            <Input
              type="password"
              :model-value="store.config.ai.openai_api_key || ''"
              placeholder="sk-..."
              @update:model-value="v => store.config.ai.openai_api_key = v"
            />
          </div>
          <div class="space-y-1">
            <Label>Base URL</Label>
            <Input
              :model-value="store.config.ai.openai_base_url || ''"
              placeholder="https://api.openai.com/v1"
              @update:model-value="v => store.config.ai.openai_base_url = v"
            />
          </div>
        </template>

        <!-- Claude Config -->
        <template v-if="isClaude">
          <div class="space-y-1">
            <Label>API Key</Label>
            <Input
              type="password"
              :model-value="store.config.ai.claude_api_key || ''"
              placeholder="sk-ant-..."
              @update:model-value="v => store.config.ai.claude_api_key = v"
            />
          </div>
          <div class="space-y-1">
            <Label>Base URL</Label>
            <Input
              :model-value="store.config.ai.claude_base_url || ''"
              placeholder="https://api.anthropic.com"
              @update:model-value="v => store.config.ai.claude_base_url = v"
            />
          </div>
        </template>

        <!-- Custom Config -->
        <template v-if="isCustom">
          <div class="space-y-1">
            <Label>名称</Label>
            <Input
              :model-value="store.config.ai.custom_name || ''"
              placeholder="自定义提供商名称"
              @update:model-value="v => store.config.ai.custom_name = v"
            />
          </div>
          <div class="space-y-1">
            <Label>Base URL</Label>
            <Input
              :model-value="store.config.ai.custom_base_url || ''"
              placeholder="https://..."
              @update:model-value="v => store.config.ai.custom_base_url = v"
            />
          </div>
          <div class="space-y-1">
            <Label>API Key</Label>
            <Input
              type="password"
              :model-value="store.config.ai.custom_api_key || ''"
              placeholder="API Key"
              @update:model-value="v => store.config.ai.custom_api_key = v"
            />
          </div>
          <div class="space-y-1">
            <Label>模型名称</Label>
            <Input
              :model-value="store.config.ai.custom_model || ''"
              placeholder="模型名称"
              @update:model-value="v => store.config.ai.custom_model = v"
            />
          </div>
        </template>
        <div class="flex items-center gap-2 pt-2">
          <Button :disabled="aiSaving" @click="saveAiSettings">
            {{ aiSaving ? '保存中...' : aiSaved ? '已保存 ✓' : '保存配置' }}
          </Button>
        </div>
      </CardContent>
    </Card>

    <!-- Privacy -->
    <Card>
      <CardHeader>
        <div class="flex items-center gap-2">
          <Shield class="h-4 w-4 text-primary" />
          <CardTitle class="text-base">隐私</CardTitle>
        </div>
        <CardDescription>隐私保护相关设置</CardDescription>
      </CardHeader>
      <CardContent class="space-y-4">
        <div class="flex items-center justify-between">
          <Label>排除私密文件</Label>
          <Switch
            :model-value="store.config.privacy.exclude_private"
            @update:model-value="(v: boolean) => updatePrivacyField('exclude_private', v)"
          />
        </div>
        <div class="flex items-center justify-between">
          <Label>排除系统文件</Label>
          <Switch
            :model-value="store.config.privacy.exclude_system"
            @update:model-value="(v: boolean) => updatePrivacyField('exclude_system', v)"
          />
        </div>
        <div class="flex items-center justify-between">
          <Label>日志保留时间</Label>
          <Select
            :model-value="String(store.config.privacy.log_retention_days)"
            @update:model-value="(v: string) => updatePrivacyField('log_retention_days', Number(v))"
          >
            <SelectTrigger class="w-[120px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem v-for="opt in retentionOptions" :key="opt.value" :value="String(opt.value)">
                {{ opt.label }}
              </SelectItem>
            </SelectContent>
          </Select>
        </div>
      </CardContent>
    </Card>

    <!-- Danger Zone -->
    <Card class="border-destructive">
      <CardHeader>
        <div class="flex items-center gap-2">
          <AlertTriangle class="h-4 w-4 text-destructive" />
          <CardTitle class="text-base text-destructive">危险区域</CardTitle>
        </div>
        <CardDescription>以下操作不可撤销，请谨慎操作</CardDescription>
      </CardHeader>
      <CardContent>
        <Button variant="destructive" @click="resetDialogOpen = true">
          <AlertTriangle class="h-4 w-4 mr-2" />
          重置数据库
        </Button>
        <p class="text-xs text-muted-foreground mt-2">清除所有扫描数据、分类规则和操作日志，恢复到初始状态。</p>
      </CardContent>
    </Card>

    <!-- Reset Confirm Dialog -->
    <Dialog v-model:open="resetDialogOpen">
      <DialogContent>
        <DialogHeader>
          <DialogTitle>确认重置</DialogTitle>
          <DialogDescription>
            此操作将清除所有数据（扫描结果、分类规则、标签、操作日志等），且不可恢复。确定要继续吗？
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" @click="resetDialogOpen = false">取消</Button>
          <Button variant="destructive" @click="resetDatabase">确认重置</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  </div>
</template>
