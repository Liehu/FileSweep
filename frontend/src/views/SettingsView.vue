<script setup lang="ts">
import { ref, reactive, onMounted } from 'vue'
import {
  NButton,
  NInput,
  NSelect,
  NSwitch,
  NModal,
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

const saving = ref(false)
const showResetModal = ref(false)
const resetting = ref(false)

async function confirmReset() {
  resetting.value = true
  try {
    await axios.post('/api/reset-db')
    message.success('数据库已重置')
    showResetModal.value = false
  } catch {
    message.error('重置数据库失败')
  } finally {
    resetting.value = false
  }
}

async function saveSettings() {
  saving.value = true
  try {
    await axios.put('/api/settings', {
      rules,
      privacy,
      ai: aiSettings,
    })
    message.success('设置已保存')
  } catch {
    message.error('保存设置失败')
  } finally {
    saving.value = false
  }
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

onMounted(async () => {
  try {
    const resp = await axios.get('/api/settings')
    const data = resp.data
    if (data.rules) Object.assign(rules, data.rules)
    if (data.privacy) Object.assign(privacy, data.privacy)
    if (data.ai) Object.assign(aiSettings, data.ai)
  } catch { /* use defaults */ }
})
</script>

<template>
  <div class="settings-view">
    <div class="settings-header">
      <h2 class="page-title">设置</h2>
      <n-button type="primary" :loading="saving" @click="saveSettings">
        保存设置
      </n-button>
    </div>

    <!-- Automation Options -->
    <div class="settings-card">
      <h3 class="card-title">自动化选项</h3>
      <p class="card-desc">配置文件整理的自动化行为</p>

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

    <!-- Danger Zone -->
    <div class="settings-card danger-card">
      <h3 class="card-title">危险操作</h3>
      <p class="card-desc">重置数据库将清除所有文件记录和操作日志</p>
      <n-button type="error" ghost @click="showResetModal = true">重置数据库</n-button>
    </div>

    <!-- Reset Confirm Modal -->
    <n-modal v-model:show="showResetModal" preset="dialog" title="确认重置数据库" type="error"
      positive-text="确认重置" negative-text="取消"
      :positive-button-props="{ disabled: resetting }"
      @positive-click="confirmReset"
    >
      <p>此操作将清除所有文件记录和操作日志，且不可恢复。确定要继续吗？</p>
    </n-modal>
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

.danger-card { border: 1px solid #fecaca; }
.danger-card .card-title { color: #dc2626; }
</style>
