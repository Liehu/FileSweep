<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { NSelect, useMessage } from 'naive-ui'
import axios from 'axios'

interface CatalogEntry {
  id: string
  name: string
  description: string
  homepageUrl: string
  downloadUrl: string
  latestVersion: string
  category: string
  functionalCategory: string
  aiConfidence: number
  aiProvider: string
  metaUpdatedAt: string
  needsReview: boolean
  tags: string[]
  license: string
}

const message = useMessage()
const entries = ref<CatalogEntry[]>([])
const loading = ref(false)
const searchText = ref('')
const exportMenuVisible = ref(false)

// Edit modal state
const editing = ref(false)
const editForm = ref<CatalogEntry>({
  id: '', name: '', description: '', homepageUrl: '', downloadUrl: '',
  latestVersion: '', category: '', functionalCategory: '', aiConfidence: 0,
  aiProvider: '', metaUpdatedAt: '', needsReview: false, tags: [], license: '',
})

// Functional category options
const funcCategoryNames = ref<string[]>([])
const funcCategoryOptions = computed(() => {
  const opts = funcCategoryNames.value.map(n => ({ label: n, value: n }))
  if (editForm.value.functionalCategory && !funcCategoryNames.value.includes(editForm.value.functionalCategory)) {
    opts.unshift({ label: editForm.value.functionalCategory, value: editForm.value.functionalCategory })
  }
  return opts
})

async function fetchFuncCategories() {
  try {
    const resp = await axios.get('/api/func-categories')
    const raw: any[] = resp.data.data ?? []
    funcCategoryNames.value = raw.map((c: any) => c.name)
  } catch { /* ignore */ }
}

onMounted(() => { fetchCatalog(); fetchFuncCategories() })

async function fetchCatalog() {
  loading.value = true
  try {
    const params: Record<string, unknown> = { page: 1, page_size: 200 }
    if (searchText.value) params.search = searchText.value
    const resp = await axios.get('/api/catalog', { params })
    const body = resp.data
    const raw: any[] = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
    entries.value = raw.map(e => ({
      ...e,
      homepageUrl: e.homepageUrl ?? e.homepage_url,
      downloadUrl: e.downloadUrl ?? e.download_url,
      latestVersion: e.latestVersion ?? e.latest_version,
      functionalCategory: e.functionalCategory ?? e.functional_category,
      aiConfidence: e.aiConfidence ?? e.ai_confidence,
      aiProvider: e.aiProvider ?? e.ai_provider,
      metaUpdatedAt: e.metaUpdatedAt ?? e.meta_updated_at,
      needsReview: e.needsReview ?? e.needs_review,
      tags: Array.isArray(e.tags) ? e.tags : [],
      license: e.license ?? '',
    }))
  } catch {
    message.error('获取软件目录失败')
  } finally {
    loading.value = false
  }
}

async function deleteEntry(id: string) {
  try {
    await axios.delete(`/api/catalog/${id}`)
    entries.value = entries.value.filter(e => e.id !== id)
    message.success('已删除')
  } catch {
    message.error('删除失败')
  }
}

function openEdit(item: CatalogEntry) {
  editForm.value = { ...item, tags: [...(item.tags || [])] }
  editing.value = true
}

async function saveEdit() {
  try {
    const form = editForm.value
    await axios.put(`/api/catalog/${form.id}`, {
      name: form.name,
      description: form.description,
      functionalCategory: form.functionalCategory,
      latestVersion: form.latestVersion,
      homepageUrl: form.homepageUrl,
      downloadUrl: form.downloadUrl,
      license: form.license,
      tags: form.tags,
      needsReview: false,
    })
    const target = entries.value.find(e => e.id === form.id)
    if (target) {
      Object.assign(target, { ...form, needsReview: false })
    }
    editing.value = false
    message.success('已保存')
  } catch {
    message.error('保存失败')
  }
}

function formatDate(dateStr: string): string {
  if (!dateStr || dateStr === '0001-01-01T00:00:00Z') return '-'
  return new Date(dateStr).toLocaleDateString('zh-CN')
}

function handleSearch() {
  fetchCatalog()
}

function exportCatalog(format: string) {
  window.open(`/api/catalog/export?format=${format}`, '_blank')
  exportMenuVisible.value = false
}
</script>

<template>
  <div class="catalog-view">
    <h2 class="page-title">软件目录</h2>

    <div class="controls-bar">
      <div class="controls-left">
        <input
          v-model="searchText"
          placeholder="搜索软件名称、描述..."
          class="search-input"
          @keyup.enter="handleSearch"
        />
        <button class="btn primary" @click="handleSearch">搜索</button>
      </div>
      <div class="controls-right">
        <div class="export-wrapper">
          <button class="btn export-btn" @click="exportMenuVisible = !exportMenuVisible">
            <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
              <polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/>
            </svg>
            导出
            <svg width="10" height="10" viewBox="0 0 12 12" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M2 4l4 4 4-4"/>
            </svg>
          </button>
          <div class="export-menu" v-if="exportMenuVisible" @mouseleave="exportMenuVisible = false">
            <button class="export-item" @click="exportCatalog('csv')">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 21V9"/>
              </svg>
              导出 CSV
            </button>
            <button class="export-item" @click="exportCatalog('obsidian')">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
                <polyline points="14 2 14 8 20 8"/><line x1="16" y1="13" x2="8" y2="13"/>
                <line x1="16" y1="17" x2="8" y2="17"/><polyline points="10 9 9 9 8 9"/>
              </svg>
              导出 Obsidian MD
            </button>
          </div>
        </div>
      </div>
    </div>

    <div class="table-area">
      <table class="catalog-table">
        <thead>
          <tr>
            <th>软件名</th>
            <th>功能分类</th>
            <th>描述</th>
            <th>标签</th>
            <th>链接</th>
            <th>更新时间</th>
            <th>最新版本</th>
            <th style="width:90px">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="entry in entries" :key="entry.id">
            <td><span class="entry-name">{{ entry.name }}</span></td>
            <td>
              <span v-if="entry.functionalCategory" class="func-tag">{{ entry.functionalCategory }}</span>
              <span v-else class="empty-tag">—</span>
            </td>
            <td><span class="entry-desc">{{ entry.description || '-' }}</span></td>
            <td class="tags-cell">
              <span
                v-for="tag in (entry.tags || []).slice(0, 3)"
                :key="tag"
                class="tag-pill"
              >{{ tag }}</span>
              <span v-if="(entry.tags || []).length > 3" class="more-tags">+{{ entry.tags.length - 3 }}</span>
              <span v-if="!entry.tags || entry.tags.length === 0" class="empty-tag">—</span>
            </td>
            <td class="link-cell">
              <a v-if="entry.homepageUrl" :href="entry.homepageUrl" target="_blank" class="entry-link-btn" title="官网">官网</a>
              <a v-if="entry.downloadUrl" :href="entry.downloadUrl" target="_blank" class="entry-link-btn download" title="下载">下载</a>
              <span v-if="!entry.homepageUrl && !entry.downloadUrl" class="entry-local">-</span>
            </td>
            <td class="date-cell">{{ formatDate(entry.metaUpdatedAt) }}</td>
            <td>
              <span v-if="entry.latestVersion" class="version-badge">{{ entry.latestVersion }}</span>
              <span v-else>-</span>
            </td>
            <td>
              <div class="action-btns">
                <button class="btn-edit" @click="openEdit(entry)" title="编辑">
                  <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M8.5 1.5l2 2L4 10H2v-2l6.5-6.5z" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                </button>
                <button class="btn-delete" @click="deleteEntry(entry.id)" title="删除">
                  <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M2 3h8M5 3V2h2v1M3 3l.5 7h5L9 3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
                </button>
              </div>
            </td>
          </tr>
          <tr v-if="loading">
            <td colspan="8" class="empty-cell">正在加载...</td>
          </tr>
          <tr v-else-if="entries.length === 0">
            <td colspan="8" class="empty-cell">{{ searchText ? '未找到匹配的软件' : '暂无软件目录数据，请先进行 AI 丰富' }}</td>
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
              <n-select
                v-model:value="editForm.functionalCategory"
                :options="funcCategoryOptions"
                filterable
                clearable
                tag
                placeholder="搜索或输入分类"
                style="width: 100%"
              />
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
.catalog-view { display: flex; flex-direction: column; gap: 16px; }
.page-title { font-size: 20px; font-weight: 700; color: #1f2937; margin: 0; }

.controls-bar {
  display: flex; align-items: center; justify-content: space-between;
  background: #fff; padding: 12px 16px; border-radius: 8px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.controls-left { display: flex; gap: 8px; align-items: center; }
.controls-right { display: flex; gap: 8px; align-items: center; }

.search-input {
  padding: 8px 14px; border: 0.5px solid #d1d5db; border-radius: 6px;
  font-size: 13px; outline: none; width: 300px;
}
.search-input:focus { border-color: #185FA5; }

.btn {
  display: inline-flex; align-items: center; gap: 5px;
  padding: 6px 14px; border: 0.5px solid #d1d5db; border-radius: 6px;
  background: #fff; font-size: 12px; color: #374151; cursor: pointer;
}
.btn.primary { background: #185FA5; color: #fff; border-color: #185FA5; }
.btn.primary:hover { background: #144e8a; }

.export-wrapper { position: relative; }
.export-btn { gap: 5px; }
.export-btn:hover { background: #f9fafb; }
.export-menu {
  position: absolute; right: 0; top: calc(100% + 4px);
  background: #fff; border: 1px solid #e5e7eb; border-radius: 8px;
  box-shadow: 0 4px 16px rgba(0,0,0,0.1); z-index: 50; min-width: 180px; overflow: hidden;
}
.export-item {
  display: flex; align-items: center; gap: 8px;
  width: 100%; padding: 10px 14px; border: none; background: none;
  font-size: 13px; color: #374151; cursor: pointer; text-align: left;
}
.export-item:hover { background: #f3f4f6; }
.export-item + .export-item { border-top: 1px solid #f3f4f6; }

.table-area {
  background: #fff; border-radius: 8px; box-shadow: 0 1px 2px rgba(0,0,0,0.05);
  overflow: hidden;
}

.catalog-table { width: 100%; border-collapse: collapse; font-size: 13px; }
.catalog-table th {
  text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600;
  color: #9ca3af; text-transform: uppercase; background: #fafafa;
  border-bottom: 0.5px solid #e5e7eb;
}
.catalog-table td { padding: 8px 12px; border-bottom: 0.5px solid #f3f4f6; }
.catalog-table tr:hover { background: #f9fafb; }

.entry-name { font-weight: 600; color: #1f2937; }
.entry-desc { color: #4b5563; font-size: 12px; }

.func-tag {
  display: inline-block; padding: 2px 8px; border-radius: 4px;
  font-size: 11px; font-weight: 500; background: #F3F4F6; color: #374151;
  border: 1px solid #E5E7EB; white-space: nowrap;
}

.empty-tag { color: #D1D5DB; font-size: 12px; }

.tags-cell { white-space: nowrap; max-width: 180px; }
.tag-pill {
  display: inline-block; padding: 1px 7px; border-radius: 10px; font-size: 11px;
  background: #E6F1FB; color: #185FA5; margin-right: 3px; margin-bottom: 2px;
}
.more-tags { font-size: 11px; color: #9ca3af; }

.link-cell { white-space: nowrap; }
.entry-link-btn {
  display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 11px;
  font-weight: 500; text-decoration: none; margin-right: 4px;
  background: #E6F1FB; color: #185FA5;
}
.entry-link-btn:hover { background: #d0e4f7; }
.entry-link-btn.download { background: #EAF3DE; color: #3B6D11; }
.entry-link-btn.download:hover { background: #d8ebc8; }
.entry-local { color: #9ca3af; font-size: 12px; }
.date-cell { color: #6b7280; font-size: 12px; }

.version-badge {
  display: inline-block; padding: 2px 8px; border-radius: 4px;
  font-size: 11px; font-weight: 600; background: #E6F1FB; color: #185FA5;
}

.action-btns { display: flex; gap: 4px; }

.btn-edit {
  background: none; border: none; cursor: pointer; color: #d1d5db;
  padding: 4px; border-radius: 4px; display: flex; align-items: center;
}
.btn-edit:hover { background: #eff6ff; color: #185FA5; }

.btn-delete {
  background: none; border: none; cursor: pointer; color: #d1d5db;
  padding: 4px; border-radius: 4px; display: flex; align-items: center;
}
.btn-delete:hover { background: #fee2e2; color: #A32D2D; }

.empty-cell { text-align: center; padding: 40px 20px !important; color: #9ca3af; }

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
</style>
