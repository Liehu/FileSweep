<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import {
  NButton,
  NInput,
  NSelect,
  NSwitch,
  NDivider,
  useMessage,
} from 'naive-ui'
import axios from 'axios'

const message = useMessage()

// Rules
const rules = reactive({
  autoCategorize: true,
  autoDuplicate: true,
  keepNewestVersion: true,
  deleteEmptyDirs: false,
  moveToRecycleBin: true,
  minFileSize: 0,
  maxFileSize: 0,
  ignorePatterns: '',
})

// Privacy
const privacy = reactive({
  shareHashes: false,
  shareMetadata: false,
  analyticsEnabled: false,
  logRetentionDays: 30,
})

// AI Provider
const aiSettings = reactive({
  provider: 'ollama',
  ollamaUrl: 'http://localhost:11434',
  openaiKey: '',
  openaiBaseUrl: '',
  claudeKey: '',
  claudeBaseUrl: '',
  customName: '',
  customUrl: '',
  customKey: '',
  customModel: '',
  model: '',
})

const providerOptions = [
  { label: 'Ollama (本地)', value: 'ollama' },
  { label: 'OpenAI', value: 'openai' },
  { label: 'Claude', value: 'claude' },
  { label: '自定义 (OpenAI 兼容)', value: 'custom' },
]

const retentionOptions = [
  { label: '7 天', value: 7 },
  { label: '30 天', value: 30 },
  { label: '90 天', value: 90 },
  { label: '永久', value: 0 },
]

// Organize rules
const organizeRules = reactive([
  { id: 1, pattern: '*.exe, *.msi, *.dmg', target: '安装包/', enabled: true },
  { id: 2, pattern: '*.pdf, *.doc, *.docx, *.xls, *.xlsx', target: '文档/', enabled: true },
  { id: 3, pattern: '*.zip, *.rar, *.7z, *.tar.gz', target: '压缩包/', enabled: true },
  { id: 4, pattern: '*.py, *.sh, *.bat, *.ps1', target: '脚本/', enabled: true },
  { id: 5, pattern: '*.iso, *.img', target: '镜像/', enabled: false },
])

const saving = ref(false)

async function saveSettings() {
  saving.value = true
  try {
    await axios.put('/api/settings', {
      rules,
      privacy,
      ai: aiSettings,
      organize_rules: organizeRules,
    })
    message.success('设置已保存')
  } catch {
    message.error('保存设置失败')
  } finally {
    saving.value = false
  }
}

function addOrganizeRule() {
  organizeRules.push({
    id: Date.now(),
    pattern: '',
    target: '',
    enabled: true,
  })
}

function removeOrganizeRule(index: number) {
  organizeRules.splice(index, 1)
}

function resetRules() {
  rules.autoCategorize = true
  rules.autoDuplicate = true
  rules.keepNewestVersion = true
  rules.deleteEmptyDirs = false
  rules.moveToRecycleBin = true
  rules.minFileSize = 0
  rules.maxFileSize = 0
  rules.ignorePatterns = ''
  message.info('已重置为默认规则')
}

// --- Category Management (rules.yaml) ---
interface RuleCategory {
  name: string
  target_path: string
  extensions: string[]
  name_keywords: string[]
  sub_categories: RuleCategory[]
}

const categories = ref<RuleCategory[]>([])
const catLoading = ref(false)

onMounted(async () => {
  try {
    const resp = await axios.get('/api/settings')
    const data = resp.data
    if (data.rules) Object.assign(rules, data.rules)
    if (data.privacy) Object.assign(privacy, data.privacy)
    if (data.ai) Object.assign(aiSettings, data.ai)
  } catch { /* use defaults */ }
  await fetchRules()
})

async function fetchRules() {
  catLoading.value = true
  try {
    const resp = await axios.get('/api/rules')
    categories.value = resp.data.data ?? []
  } catch { categories.value = [] }
  finally { catLoading.value = false }
}

function addCategory() {
  categories.value.push({
    name: '', target_path: '', extensions: [], name_keywords: [], sub_categories: [],
  })
}

async function saveCategory(cat: RuleCategory) {
  if (!cat.name) { message.warning('分类名称不能为空'); return }
  // Normalize from editing string or existing array
  const extSource = (cat as any)._extStr ?? cat.extensions
  cat.extensions = (typeof extSource === 'string' ? extSource : Array.isArray(extSource) ? extSource.join(',') : '')
    .split(',').map((s: string) => s.trim()).filter(Boolean)
  const kwSource = (cat as any)._kwStr ?? cat.name_keywords
  cat.name_keywords = (typeof kwSource === 'string' ? kwSource : Array.isArray(kwSource) ? kwSource.join(',') : '')
    .split(',').map((s: string) => s.trim()).filter(Boolean)
  delete (cat as any)._extStr
  delete (cat as any)._kwStr
  try {
    await axios.put('/api/rules', categories.value)
    message.success('分类规则已保存')
  } catch { message.error('保存分类失败') }
}

async function deleteCategory(index: number) {
  categories.value.splice(index, 1)
  try {
    await axios.put('/api/rules', categories.value)
    message.success('分类已删除')
  } catch { message.error('删除分类失败') }
}

function getCatExtInput(cat: RuleCategory): string {
  if ((cat as any)._extStr !== undefined) return (cat as any)._extStr
  return Array.isArray(cat.extensions) ? cat.extensions.join(', ') : ''
}

function setCatExtStr(cat: RuleCategory, val: string) {
  ;(cat as any)._extStr = val
}

function getCatKwInput(cat: RuleCategory): string {
  if ((cat as any)._kwStr !== undefined) return (cat as any)._kwStr
  return Array.isArray(cat.name_keywords) ? cat.name_keywords.join(', ') : ''
}

function setCatKwStr(cat: RuleCategory, val: string) {
  ;(cat as any)._kwStr = val
}
</script>

<template>
  <div class="settings-view">
    <div class="settings-header">
      <h2 class="page-title">设置</h2>
      <n-button type="primary" :loading="saving" @click="saveSettings">
        保存设置
      </n-button>
    </div>

    <!-- Organize Rules -->
    <div class="settings-card">
      <h3 class="card-title">整理规则</h3>
      <p class="card-desc">配置文件分类和整理的自动化规则</p>

      <div class="rules-table">
        <div class="rules-header">
          <span class="col-pattern">文件模式</span>
          <span class="col-target">目标文件夹</span>
          <span class="col-enabled">启用</span>
          <span class="col-action">操作</span>
        </div>
        <div
          v-for="(rule, index) in organizeRules"
          :key="rule.id"
          class="rule-row"
        >
          <div class="col-pattern">
            <n-input v-model:value="rule.pattern" placeholder="*.exe, *.msi" size="small" />
          </div>
          <div class="col-target">
            <n-input v-model:value="rule.target" placeholder="安装包/" size="small" />
          </div>
          <div class="col-enabled">
            <n-switch v-model:value="rule.enabled" size="small" />
          </div>
          <div class="col-action">
            <button class="btn-remove" @click="removeOrganizeRule(index)" title="删除规则">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M18 6L6 18M6 6l12 12" />
              </svg>
            </button>
          </div>
        </div>
      </div>
      <n-button dashed block @click="addOrganizeRule" style="margin-top: 8px">
        + 添加规则
      </n-button>

      <n-divider />

      <h4 class="sub-title">自动化选项</h4>
      <div class="option-grid">
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">自动分类</span>
            <span class="option-desc">扫描后自动将文件归入对应类别</span>
          </div>
          <n-switch v-model:value="rules.autoCategorize" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">自动去重</span>
            <span class="option-desc">自动标记重复文件，保留最新版本</span>
          </div>
          <n-switch v-model:value="rules.autoDuplicate" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">保留最新版本</span>
            <span class="option-desc">多版本文件仅保留最新版本</span>
          </div>
          <n-switch v-model:value="rules.keepNewestVersion" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">移至回收站</span>
            <span class="option-desc">删除文件时移至回收站而非永久删除</span>
          </div>
          <n-switch v-model:value="rules.moveToRecycleBin" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">删除空目录</span>
            <span class="option-desc">整理后自动删除空目录</span>
          </div>
          <n-switch v-model:value="rules.deleteEmptyDirs" />
        </div>
      </div>

      <div class="option-actions">
        <n-button size="small" @click="resetRules">恢复默认</n-button>
      </div>
    </div>

    <!-- AI Provider -->
    <div class="settings-card">
      <h3 class="card-title">AI 提供者</h3>
      <p class="card-desc">配置 AI 丰富功能所使用的提供者</p>

      <div class="form-grid">
        <div class="form-item">
          <label class="form-label">提供者</label>
          <n-select v-model:value="aiSettings.provider" :options="providerOptions" />
        </div>
        <div class="form-item" v-if="aiSettings.provider === 'ollama'">
          <label class="form-label">Ollama 地址</label>
          <n-input v-model:value="aiSettings.ollamaUrl" placeholder="http://localhost:11434" />
        </div>
        <div class="form-item" v-if="aiSettings.provider === 'ollama'">
          <label class="form-label">模型名称</label>
          <n-input v-model:value="aiSettings.model" placeholder="llama3" />
        </div>
        <div class="form-item" v-if="aiSettings.provider === 'openai'">
          <label class="form-label">API Key</label>
          <n-input v-model:value="aiSettings.openaiKey" type="password" show-password-on="click" placeholder="sk-..." />
        </div>
        <div class="form-item" v-if="aiSettings.provider === 'openai'">
          <label class="form-label">Base URL（可选）</label>
          <n-input v-model:value="aiSettings.openaiBaseUrl" placeholder="https://api.openai.com/v1" />
        </div>
        <div class="form-item" v-if="aiSettings.provider === 'claude'">
          <label class="form-label">API Key</label>
          <n-input v-model:value="aiSettings.claudeKey" type="password" show-password-on="click" placeholder="sk-ant-..." />
        </div>
        <div class="form-item" v-if="aiSettings.provider === 'claude'">
          <label class="form-label">Base URL（可选）</label>
          <n-input v-model:value="aiSettings.claudeBaseUrl" placeholder="https://api.anthropic.com" />
        </div>
        <template v-if="aiSettings.provider === 'custom'">
          <div class="form-item">
            <label class="form-label">提供者名称</label>
            <n-input v-model:value="aiSettings.customName" placeholder="如：DeepSeek" />
          </div>
          <div class="form-item">
            <label class="form-label">API URL</label>
            <n-input v-model:value="aiSettings.customUrl" placeholder="https://api.example.com/v1" />
          </div>
          <div class="form-item">
            <label class="form-label">API Key</label>
            <n-input v-model:value="aiSettings.customKey" type="password" show-password-on="click" placeholder="sk-..." />
          </div>
          <div class="form-item">
            <label class="form-label">模型名称</label>
            <n-input v-model:value="aiSettings.customModel" placeholder="如：deepseek-chat" />
          </div>
        </template>
      </div>
    </div>

    <!-- Privacy -->
    <div class="settings-card">
      <h3 class="card-title">隐私设置</h3>
      <p class="card-desc">控制数据共享和日志记录行为</p>

      <div class="option-grid">
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">共享文件哈希</span>
            <span class="option-desc">将文件哈希发送至远程服务用于识别</span>
          </div>
          <n-switch v-model:value="privacy.shareHashes" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">共享元数据</span>
            <span class="option-desc">共享文件名、大小等非敏感元数据</span>
          </div>
          <n-switch v-model:value="privacy.shareMetadata" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">使用统计</span>
            <span class="option-desc">发送匿名使用统计以改进产品</span>
          </div>
          <n-switch v-model:value="privacy.analyticsEnabled" />
        </div>
        <div class="option-item">
          <div class="option-info">
            <span class="option-label">日志保留</span>
            <span class="option-desc">操作日志的保留时间</span>
          </div>
          <n-select
            v-model:value="privacy.logRetentionDays"
            :options="retentionOptions"
            style="width: 120px"
          />
        </div>
      </div>
    </div>

    <!-- Category Management (rules.yaml) -->
    <div class="settings-card">
      <div class="card-header-row">
        <div>
          <h3 class="card-title">分类管理</h3>
          <p class="card-desc">管理 rules.yaml 中的分类规则，修改后即时生效</p>
        </div>
        <n-button size="small" @click="addCategory">+ 添加分类</n-button>
      </div>

      <div class="cat-table" v-if="categories.length > 0">
        <div class="cat-header">
          <span class="cat-col-name">分类名称</span>
          <span class="cat-col-path">目标路径</span>
          <span class="cat-col-ext">文件后缀</span>
          <span class="cat-col-kw">关键词</span>
          <span class="cat-col-action">操作</span>
        </div>
        <div v-for="(cat, index) in categories" :key="index" class="cat-row">
          <div class="cat-col-name">
            <n-input v-model:value="cat.name" placeholder="如：安装包" size="small" />
          </div>
          <div class="cat-col-path">
            <n-input v-model:value="cat.target_path" placeholder="如：Installers" size="small" />
          </div>
          <div class="cat-col-ext">
            <n-input
              :value="getCatExtInput(cat)"
              @update:value="(v: string) => setCatExtStr(cat, v)"
              placeholder=".exe, .msi"
              size="small"
            />
          </div>
          <div class="cat-col-kw">
            <n-input
              :value="getCatKwInput(cat)"
              @update:value="(v: string) => setCatKwStr(cat, v)"
              placeholder="setup, install"
              size="small"
            />
          </div>
          <div class="cat-col-action">
            <n-button size="tiny" type="primary" @click="saveCategory(cat)">保存</n-button>
            <n-button size="tiny" tertiary type="error" @click="deleteCategory(index)" style="margin-left:4px">删除</n-button>
          </div>
        </div>
      </div>
      <div v-else class="cat-empty">暂无分类规则，点击上方按钮添加</div>
    </div>
  </div>
</template>

<style scoped>
.settings-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.settings-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.page-title {
  font-size: 20px;
  font-weight: 700;
  color: #1f2937;
  margin: 0;
}

.settings-card {
  background: #fff;
  border-radius: 8px;
  padding: 20px 24px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.card-title {
  font-size: 16px;
  font-weight: 700;
  color: #1f2937;
  margin: 0 0 4px;
}

.card-desc {
  font-size: 13px;
  color: #9ca3af;
  margin: 0 0 16px;
}

.sub-title {
  font-size: 14px;
  font-weight: 600;
  color: #374151;
  margin: 0 0 12px;
}

/* Rules table */
.rules-table {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.rules-header {
  display: flex;
  gap: 8px;
  padding: 6px 8px;
  font-size: 12px;
  font-weight: 600;
  color: #9ca3af;
  text-transform: uppercase;
}

.rule-row {
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 4px 8px;
  border-radius: 6px;
  transition: background 0.1s;
}

.rule-row:hover {
  background: #f9fafb;
}

.col-pattern {
  flex: 2;
}

.col-target {
  flex: 1.5;
}

.col-enabled {
  width: 60px;
  display: flex;
  justify-content: center;
}

.col-action {
  width: 36px;
  display: flex;
  justify-content: center;
}

.btn-remove {
  background: none;
  border: none;
  cursor: pointer;
  color: #d1d5db;
  padding: 4px;
  border-radius: 4px;
  display: flex;
  align-items: center;
}

.btn-remove:hover {
  background: #fee2e2;
  color: var(--color-danger);
}

/* Option grid */
.option-grid {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.option-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 0;
  border-bottom: 1px solid #f3f4f6;
}

.option-item:last-child {
  border-bottom: none;
}

.option-info {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.option-label {
  font-size: 14px;
  font-weight: 500;
  color: #374151;
}

.option-desc {
  font-size: 12px;
  color: #9ca3af;
}

.option-actions {
  margin-top: 12px;
  display: flex;
  justify-content: flex-end;
}

/* Form grid */
.form-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
  gap: 16px;
}

.form-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.form-label {
  font-size: 13px;
  font-weight: 500;
  color: #4b5563;
}

.card-header-row {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 16px;
}

.cat-table { display: flex; flex-direction: column; gap: 4px; }
.cat-header { display: flex; gap: 8px; padding: 6px 8px; font-size: 11px; font-weight: 600; color: #9ca3af; text-transform: uppercase; }
.cat-row { display: flex; gap: 8px; align-items: center; padding: 4px 8px; border-radius: 6px; }
.cat-row:hover { background: #f9fafb; }
.cat-col-name { flex: 1.2; }
.cat-col-path { flex: 1; }
.cat-col-ext { flex: 1.5; }
.cat-col-kw { flex: 1; }
.cat-col-action { width: 120px; display: flex; align-items: center; }
.cat-empty { text-align: center; color: #9ca3af; padding: 24px; font-size: 13px; }
</style>
