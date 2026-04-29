<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { NSelect, useMessage } from 'naive-ui'
import axios from 'axios'

const message = useMessage()

const selectedProvider = ref('offline')
const running = ref(false)
const progress = ref({ total: 0, done: 0, failed: 0, percent: 0 })
const results = ref<EnrichResult[]>([])
const loading = ref(false)

const providerOptions = [
  { label: 'Offline (本地数据库)', value: 'offline' },
  { label: 'Ollama (本地模型)', value: 'ollama' },
  { label: 'OpenAI', value: 'openai' },
  { label: 'Claude', value: 'claude' },
  { label: '自定义', value: 'custom' },
]

interface EnrichResult {
  id: string
  name: string
  description: string
  functionalCategory: string
  latestVersion: string
  homepageUrl: string
  downloadUrl: string
  confidence: number
  needsReview: boolean
  status: 'pending' | 'done' | 'error'
  tags: string[]
  license: string
  aiProvider: string
}

// Edit modal state
const editing = ref(false)
const editForm = ref<EnrichResult>({
  id: '', name: '', description: '', functionalCategory: '',
  latestVersion: '', homepageUrl: '', downloadUrl: '',
  confidence: 0, needsReview: false, status: 'done',
  tags: [], license: '', aiProvider: '',
})

let ws: WebSocket | null = null
let pollTimer: ReturnType<typeof setInterval> | null = null

// Predefined tags for suggestions
const predefinedTags = ref<string[]>([])

async function fetchPredefinedTags() {
  try {
    const resp = await axios.get('/api/tags')
    const raw: any[] = resp.data.data ?? []
    predefinedTags.value = raw.map((t: any) => t.name)
  } catch { /* ignore */ }
}

onMounted(() => {
  connectWebSocket()
  fetchCatalog()
  fetchPredefinedTags()
  pollTimer = setInterval(() => {
    if (running.value) fetchCatalog()
  }, 3000)
})

onUnmounted(() => {
  if (ws) ws.close()
  if (pollTimer) clearInterval(pollTimer)
})

function connectWebSocket() {
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${protocol}//${window.location.host}/ws`)

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data)
      if (data.type === 'enrich_progress') {
        const p = data.payload ?? data
        progress.value = {
          total: p.total ?? 0,
          done: p.done ?? 0,
          failed: 0,
          percent: p.total > 0 ? Math.round((p.done / p.total) * 100) : 0,
        }
      } else if (data.type === 'enrich_complete') {
        running.value = false
        progress.value.percent = 100
        const payload = data.payload ?? data
        const saved = payload.saved ?? 0
        const skipped = payload.skipped ?? 0
        if (skipped > 0) {
          message.warning(`AI 丰富完成：${saved} 个成功，${skipped} 个失败（可能限流或无结果）`)
        } else {
          message.success(`AI 丰富完成，共处理 ${payload.total} 个文件`)
        }
        fetchCatalog()
      } else if (data.type === 'enrich_error') {
        running.value = false
        const payload = data.payload ?? data
        message.error('AI 丰富失败: ' + (payload.error ?? '未知错误'))
      }
    } catch { /* ignore */ }
  }

  ws.onclose = () => { setTimeout(connectWebSocket, 3000) }
  ws.onerror = () => { /* ws will close and reconnect */ }
}

async function fetchCatalog() {
  loading.value = true
  try {
    const resp = await axios.get('/api/catalog', { params: { page: 1, page_size: 200 } })
    const body = resp.data
    const raw = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
    results.value = raw.map((e: Record<string, unknown>) => ({
      id: e.id ?? '',
      name: e.name ?? '',
      description: e.description ?? '',
      functionalCategory: (e.functionalCategory ?? e.functional_category ?? '') as string,
      latestVersion: e.latestVersion ?? e.latest_version ?? '',
      homepageUrl: e.homepageUrl ?? e.homepage_url ?? '',
      downloadUrl: e.downloadUrl ?? e.download_url ?? '',
      confidence: e.aiConfidence ?? e.ai_confidence ?? 0,
      needsReview: e.needsReview ?? e.needs_review ?? false,
      status: 'done' as const,
      tags: (Array.isArray(e.tags) ? e.tags : []) as string[],
      license: (e.license ?? '') as string,
      aiProvider: (e.aiProvider ?? e.ai_provider ?? '') as string,
    }))
  } catch { /* ignore */ }
  finally { loading.value = false }
}

async function startEnrich() {
  running.value = true
  progress.value = { total: 0, done: 0, failed: 0, percent: 0 }
  try {
    await axios.post('/api/enrich', { provider: selectedProvider.value, concurrency: 1 })
    message.info('AI 丰富任务已启动')
  } catch {
    running.value = false
    message.error('启动 AI 丰富失败')
  }
}

async function approveResult(id: string) {
  try {
    const entry = results.value.find(r => r.id === id)
    if (entry) {
      await axios.put(`/api/catalog/${id}`, {
        ...entry,
        needsReview: false,
      })
      entry.needsReview = false
      message.success('已采纳')
    }
  } catch {
    message.error('操作失败')
  }
}

async function rejectResult(id: string) {
  try {
    await axios.delete(`/api/catalog/${id}`)
    results.value = results.value.filter(r => r.id !== id)
    message.info('已拒绝并删除')
  } catch {
    message.error('操作失败')
  }
}

function openEdit(item: EnrichResult) {
  editForm.value = { ...item }
  editing.value = true
}

async function saveEdit() {
  try {
    const form = editForm.value
    await axios.put(`/api/catalog/${form.id}`, {
      description: form.description,
      functionalCategory: form.functionalCategory,
      latestVersion: form.latestVersion,
      homepageUrl: form.homepageUrl,
      downloadUrl: form.downloadUrl,
      license: form.license,
      tags: form.tags,
      needsReview: false,
    })
    const target = results.value.find(r => r.id === form.id)
    if (target) {
      Object.assign(target, form)
      target.needsReview = false
    }
    editing.value = false
    message.success('已保存')
  } catch {
    message.error('保存失败')
  }
}

const reviewItems = computed(() => results.value.filter(r => r.needsReview))
</script>

<template>
  <div class="enrich-view">
    <h2 class="page-title">AI 丰富</h2>

    <!-- Controls -->
    <div class="controls-bar">
      <div class="controls-left">
        <span class="control-label">AI 提供者:</span>
        <n-select v-model:value="selectedProvider" :options="providerOptions" style="width: 200px" :disabled="running" />
        <button class="btn primary" :disabled="running" @click="startEnrich">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><polygon points="2 1 10 6 2 11 2 1" :fill="running ? '#999' : '#fff'"/></svg>
          {{ running ? '处理中...' : '开始丰富' }}
        </button>
      </div>
    </div>

    <!-- Progress -->
    <div class="progress-card" v-if="running || progress.percent > 0">
      <div class="progress-header">
        <span class="progress-label">处理进度 ({{ progress.done }}/{{ progress.total }})</span>
      </div>
      <div class="progress-bar">
        <div class="progress-fill" :style="{ width: progress.percent + '%' }"></div>
      </div>
    </div>

    <!-- Stats -->
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-value">{{ results.length }}</div>
        <div class="stat-label">已丰富</div>
      </div>
      <div class="stat-card stat-warn">
        <div class="stat-value">{{ reviewItems.length }}</div>
        <div class="stat-label">待审查</div>
      </div>
    </div>

    <!-- Review Queue -->
    <div class="section-title" v-if="reviewItems.length > 0">待审查 ({{ reviewItems.length }})</div>
    <div class="table-area" v-if="reviewItems.length > 0">
      <table class="enrich-table">
        <thead>
          <tr>
            <th>文件名</th>
            <th>功能分类</th>
            <th>描述</th>
            <th>置信度</th>
            <th style="width:150px">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="item in reviewItems" :key="item.id">
            <td><span class="file-name">{{ item.name }}</span></td>
            <td>
              <span v-if="item.functionalCategory" class="func-tag">{{ item.functionalCategory }}</span>
              <span v-else class="empty-tag">—</span>
            </td>
            <td class="desc-cell">{{ item.description }}</td>
            <td>
              <span class="confidence" :class="{ low: item.confidence < 0.6 }">
                {{ (item.confidence * 100).toFixed(0) }}%
              </span>
            </td>
            <td>
              <div class="action-btns">
                <button class="btn small success" @click="approveResult(item.id)">采纳</button>
                <button class="btn small edit" @click="openEdit(item)">编辑</button>
                <button class="btn small danger" @click="rejectResult(item.id)">拒绝</button>
              </div>
            </td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- All results -->
    <div class="section-title">全部结果 ({{ results.length }})</div>
    <div class="table-area">
      <table class="enrich-table">
        <thead>
          <tr>
            <th>文件名</th>
            <th>功能分类</th>
            <th>描述</th>
            <th>最新版本</th>
            <th>链接</th>
            <th>状态</th>
            <th style="width:60px">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="item in results" :key="item.id">
            <td><span class="file-name">{{ item.name }}</span></td>
            <td>
              <span v-if="item.functionalCategory" class="func-tag">{{ item.functionalCategory }}</span>
              <span v-else class="empty-tag">—</span>
            </td>
            <td class="desc-cell">{{ item.description || '-' }}</td>
            <td>{{ item.latestVersion || '-' }}</td>
            <td class="link-cell">
              <a v-if="item.homepageUrl" :href="item.homepageUrl" target="_blank" class="link-btn" title="官网">官网</a>
              <a v-if="item.downloadUrl" :href="item.downloadUrl" target="_blank" class="link-btn download" title="下载">下载</a>
              <span v-if="!item.homepageUrl && !item.downloadUrl" class="no-link">-</span>
            </td>
            <td>
              <span class="status-tag" :class="item.needsReview ? 'review' : 'approved'">
                {{ item.needsReview ? '待审查' : '已采纳' }}
              </span>
            </td>
            <td>
              <button class="btn small edit" @click="openEdit(item)" title="编辑">编辑</button>
            </td>
          </tr>
          <tr v-if="results.length === 0 && !loading">
            <td colspan="7" class="empty-cell">暂无丰富结果，请先启动 AI 丰富任务</td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Edit Modal -->
    <div class="modal-overlay" v-if="editing" @click.self="editing = false">
      <div class="modal">
        <div class="modal-header">
          <h3>编辑 - {{ editForm.name }}</h3>
          <button class="modal-close" @click="editing = false">&times;</button>
        </div>
        <div class="modal-body">
          <div class="form-group">
            <label>描述</label>
            <textarea v-model="editForm.description" rows="3"></textarea>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label>功能分类</label>
              <input v-model="editForm.functionalCategory" />
            </div>
            <div class="form-group">
              <label>最新版本</label>
              <input v-model="editForm.latestVersion" />
            </div>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label>官网链接</label>
              <input v-model="editForm.homepageUrl" placeholder="https://" />
            </div>
            <div class="form-group">
              <label>下载链接</label>
              <input v-model="editForm.downloadUrl" placeholder="https://" />
            </div>
          </div>
          <div class="form-row">
            <div class="form-group">
              <label>许可证</label>
              <input v-model="editForm.license" />
            </div>
            <div class="form-group">
              <label>标签 (逗号分隔)</label>
              <input :value="editForm.tags.join(', ')" @input="editForm.tags = ($event.target as HTMLInputElement).value.split(',').map(s => s.trim()).filter(Boolean)" />
              <div class="tag-suggestions" v-if="predefinedTags.length > 0">
                <span class="suggestion-label">推荐: </span>
                <button
                  v-for="pt in predefinedTags.filter(t => !editForm.tags.includes(t))"
                  :key="pt"
                  class="tag-chip"
                  @click="editForm.tags = [...editForm.tags, pt]"
                >{{ pt }}</button>
              </div>
            </div>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn" @click="editing = false">取消</button>
          <button class="btn primary" @click="saveEdit">保存</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.enrich-view { display: flex; flex-direction: column; gap: 16px; }
.page-title { font-size: 20px; font-weight: 700; color: #1f2937; margin: 0; }

.controls-bar {
  display: flex; align-items: center; justify-content: space-between;
  background: #fff; padding: 12px 16px; border-radius: 8px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.controls-left { display: flex; gap: 10px; align-items: center; }
.control-label { font-size: 14px; font-weight: 500; color: #374151; }

.btn {
  display: inline-flex; align-items: center; gap: 4px;
  padding: 6px 14px; border: 0.5px solid #d1d5db; border-radius: 6px;
  background: #fff; font-size: 12px; color: #374151; cursor: pointer;
}
.btn.primary { background: #185FA5; color: #fff; border-color: #185FA5; }
.btn.primary:hover { background: #144e8a; }
.btn.primary:disabled { opacity: 0.6; cursor: not-allowed; }
.btn.small { padding: 3px 10px; font-size: 11px; }
.btn.success { color: #3B6D11; border-color: #86efac; }
.btn.success:hover { background: #f0fdf4; }
.btn.edit { color: #185FA5; border-color: #93c5fd; }
.btn.edit:hover { background: #eff6ff; }
.btn.danger { color: #A32D2D; border-color: #fca5a5; }
.btn.danger:hover { background: #fef2f2; }

.progress-card { background: #fff; border-radius: 8px; padding: 16px 20px; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
.progress-header { display: flex; justify-content: space-between; margin-bottom: 8px; }
.progress-label { font-size: 14px; font-weight: 500; color: #374151; }
.progress-bar { height: 16px; background: #e5e7eb; border-radius: 8px; overflow: hidden; }
.progress-fill { height: 100%; background: #185FA5; border-radius: 8px; transition: width 0.3s; }

.stats-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; }
.stat-card { background: #fff; border-radius: 8px; padding: 14px 18px; box-shadow: 0 1px 2px rgba(0,0,0,0.05); border-left: 3px solid #e5e7eb; }
.stat-card.stat-warn { border-left-color: #854F0B; }
.stat-value { font-size: 22px; font-weight: 700; color: #1f2937; }
.stat-label { font-size: 12px; color: #6b7280; margin-top: 2px; }

.section-title { font-size: 14px; font-weight: 600; color: #374151; }

.table-area { background: #fff; border-radius: 8px; box-shadow: 0 1px 2px rgba(0,0,0,0.05); overflow: hidden; }
.enrich-table { width: 100%; border-collapse: collapse; font-size: 13px; }
.enrich-table th { text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600; color: #9ca3af; text-transform: uppercase; background: #fafafa; border-bottom: 0.5px solid #e5e7eb; }
.enrich-table td { padding: 8px 12px; border-bottom: 0.5px solid #f3f4f6; }
.enrich-table tr:hover { background: #f9fafb; }

.file-name {
  font-family: monospace;
  font-size: 11px;
  color: #1f2937;
}

.func-tag {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 11px;
  font-weight: 500;
  background: #F3F4F6;
  color: #374151;
  border: 1px solid #E5E7EB;
  white-space: nowrap;
}

.empty-tag {
  color: #D1D5DB;
  font-size: 12px;
}

.desc-cell { color: #4b5563; max-width: 300px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

.confidence { font-size: 12px; font-weight: 600; color: #3B6D11; }
.confidence.low { color: #854F0B; }

.action-btns { display: flex; gap: 6px; }

.status-tag {
  display: inline-block; padding: 2px 8px; border-radius: 4px;
  font-size: 11px; font-weight: 600;
}
.status-tag.approved { background: #EAF3DE; color: #3B6D11; }
.status-tag.review { background: #FFFBEB; color: #854F0B; }

.empty-cell { text-align: center; padding: 40px 20px !important; color: #9ca3af; }

.link-cell { white-space: nowrap; }
.link-btn {
  display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 11px;
  font-weight: 500; text-decoration: none; margin-right: 4px;
  background: #E6F1FB; color: #185FA5;
}
.link-btn:hover { background: #d0e4f7; }
.link-btn.download { background: #EAF3DE; color: #3B6D11; }
.link-btn.download:hover { background: #d8ebc8; }
.no-link { color: #d1d5db; }

/* Modal */
.modal-overlay {
  position: fixed; inset: 0; background: rgba(0,0,0,0.4);
  display: flex; align-items: center; justify-content: center; z-index: 1000;
}
.modal {
  background: #fff; border-radius: 12px; width: 600px; max-width: 90vw;
  max-height: 85vh; overflow-y: auto; box-shadow: 0 20px 60px rgba(0,0,0,0.2);
}
.modal-header {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px; border-bottom: 1px solid #e5e7eb;
}
.modal-header h3 { margin: 0; font-size: 16px; font-weight: 600; color: #1f2937; }
.modal-close {
  background: none; border: none; font-size: 22px; color: #9ca3af; cursor: pointer;
  padding: 0 4px; line-height: 1;
}
.modal-close:hover { color: #374151; }
.modal-body { padding: 20px; display: flex; flex-direction: column; gap: 14px; }
.modal-footer {
  display: flex; justify-content: flex-end; gap: 8px;
  padding: 12px 20px; border-top: 1px solid #e5e7eb;
}

.form-group { display: flex; flex-direction: column; gap: 4px; flex: 1; }
.form-group label { font-size: 12px; font-weight: 600; color: #6b7280; }
.form-group input, .form-group textarea {
  padding: 8px 10px; border: 1px solid #d1d5db; border-radius: 6px;
  font-size: 13px; font-family: inherit; resize: vertical;
}
.form-group input:focus, .form-group textarea:focus {
  outline: none; border-color: #185FA5; box-shadow: 0 0 0 2px rgba(24,95,165,0.1);
}
.form-row { display: flex; gap: 12px; }

.tag-suggestions {
  display: flex; flex-wrap: wrap; gap: 4px; align-items: center; margin-top: 4px;
}
.suggestion-label { font-size: 11px; color: #9ca3af; }
.tag-chip {
  display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 11px;
  background: #E6F1FB; color: #185FA5; border: 1px solid #B3D4EE;
  cursor: pointer; line-height: 1.4;
}
.tag-chip:hover { background: #d0e4f7; }
</style>
