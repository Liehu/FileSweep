<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useMessage } from 'naive-ui'
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
}

const message = useMessage()
const entries = ref<CatalogEntry[]>([])
const loading = ref(false)
const searchText = ref('')

onMounted(() => { fetchCatalog() })

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

function formatDate(dateStr: string): string {
  if (!dateStr || dateStr === '0001-01-01T00:00:00Z') return '-'
  return new Date(dateStr).toLocaleDateString('zh-CN')
}

function handleSearch() {
  fetchCatalog()
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
    </div>

    <div class="table-area">
      <table class="catalog-table">
        <thead>
          <tr>
            <th>软件名</th>
            <th>功能分类</th>
            <th>描述</th>
            <th>链接</th>
            <th>更新时间</th>
            <th>最新版本</th>
            <th style="width:60px">操作</th>
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
              <button class="btn-delete" @click="deleteEntry(entry.id)" title="删除">
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M2 3h8M5 3V2h2v1M3 3l.5 7h5L9 3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </button>
            </td>
          </tr>
          <tr v-if="loading">
            <td colspan="7" class="empty-cell">正在加载...</td>
          </tr>
          <tr v-else-if="entries.length === 0">
            <td colspan="7" class="empty-cell">{{ searchText ? '未找到匹配的软件' : '暂无软件目录数据，请先进行 AI 丰富' }}</td>
          </tr>
        </tbody>
      </table>
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

.search-input {
  padding: 8px 14px; border: 0.5px solid #d1d5db; border-radius: 6px;
  font-size: 13px; outline: none; width: 300px;
}
.search-input:focus { border-color: #185FA5; }

.btn {
  display: inline-flex; align-items: center; gap: 4px;
  padding: 6px 14px; border: 0.5px solid #d1d5db; border-radius: 6px;
  background: #fff; font-size: 12px; color: #374151; cursor: pointer;
}
.btn.primary { background: #185FA5; color: #fff; border-color: #185FA5; }
.btn.primary:hover { background: #144e8a; }

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

.entry-link { color: #185FA5; text-decoration: none; font-size: 12px; }
.entry-link:hover { text-decoration: underline; }
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

.category-badge {
  display: inline-block; padding: 2px 8px; border-radius: 4px;
  font-size: 11px; background: #F3F4F6; color: #4b5563;
}

.btn-delete {
  background: none; border: none; cursor: pointer; color: #d1d5db;
  padding: 4px; border-radius: 4px; display: flex; align-items: center;
}
.btn-delete:hover { background: #fee2e2; color: #A32D2D; }

.empty-cell { text-align: center; padding: 40px 20px !important; color: #9ca3af; }
</style>
