<script setup lang="ts">
import { ref, onMounted, onUnmounted, computed, watch } from 'vue'
import { useRoute } from 'vue-router'
import { NSelect, useMessage } from 'naive-ui'
import { useFilesStore, type FileRecord } from '../stores/files'
import { useScanStore } from '../stores/scan'

const route = useRoute()
const store = useFilesStore()
const scanStore = useScanStore()
const message = useMessage()

const searchText = ref('')

let ws: WebSocket | null = null

onMounted(async () => {
  applyQueryParams()
  await Promise.all([store.fetchFiles(), store.fetchStats(), store.fetchSuggestions()])
  connectWebSocket()
})

onUnmounted(() => {
  if (ws) { ws.close(); ws = null }
})

function connectWebSocket() {
  if (ws) return
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${protocol}//${window.location.host}/ws`)

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data)
      if (data.type === 'clean_complete') {
        const total = data.payload?.total ?? data.total ?? 0
        message.success(`清理完成，共处理 ${total} 个操作`)
        store.fetchFiles()
        store.fetchStats()
        store.fetchSuggestions()
      } else if (data.type === 'clean_error') {
        message.error('清理失败: ' + (data.payload?.error ?? '未知错误'))
      } else if (data.type === 'scan_complete') {
        store.fetchFiles()
        store.fetchStats()
        store.fetchSuggestions()
      }
    } catch { /* ignore */ }
  }

  ws.onclose = () => { ws = null }
  ws.onerror = () => { /* ws will close */ }
}

watch(() => route.fullPath, () => {
  applyQueryParams()
})

function applyQueryParams() {
  const q = route.query
  store.setFilterDup(q.dup === '1')
  store.setFilterMultiVersion(q.mv === '1')
  if (q.cat) store.setFilterCategory(q.cat as string)
  else store.setFilterCategory('')
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

const actionOptions = [
  { label: '保留', value: 'keep' },
  { label: '删除', value: 'delete' },
  { label: '移动', value: 'move' },
  { label: '归档', value: 'archive' },
]

const catMap: Record<string, { label: string; bg: string; color: string }> = {
  安装包: { label: '安装包', bg: '#E6F1FB', color: '#185FA5' },
  installer: { label: '安装包', bg: '#E6F1FB', color: '#185FA5' },
  文档: { label: '文档', bg: '#EAF3DE', color: '#3B6D11' },
  document: { label: '文档', bg: '#EAF3DE', color: '#3B6D11' },
  压缩包: { label: '压缩包', bg: '#FAEEDA', color: '#854F0B' },
  archive: { label: '压缩包', bg: '#FAEEDA', color: '#854F0B' },
  脚本: { label: '脚本', bg: '#F3E8FF', color: '#7C3AED' },
  script: { label: '脚本', bg: '#F3E8FF', color: '#7C3AED' },
}

function getCatInfo(category: string) {
  return catMap[category] ?? { label: category || '未知', bg: '#F3F4F6', color: '#6B7280' }
}

function getCatLabel(category: string): string {
  return getCatInfo(category).label
}

function getCatStyle(category: string): Record<string, string> {
  const info = getCatInfo(category)
  return { display: 'inline-block', padding: '2px 8px', borderRadius: '4px', fontSize: '11px', fontWeight: '500', background: info.bg, color: info.color }
}

function getSuggestionText(file: FileRecord): string {
  const sug = store.suggestions.get(file.id)
  if (!sug) return ''
  if (sug.is_dup && sug.reason) {
    return '删除（' + sug.reason + '）'
  }
  if (sug.suggest && sug.suggest.startsWith('→')) {
    return sug.suggest
  }
  if (sug.target) {
    return '→ ' + sug.target + '\\' + file.name
  }
  return ''
}

function getSuggestionClass(file: FileRecord): string {
  const sug = store.suggestions.get(file.id)
  if (sug?.is_dup) return 'suggest-delete'
  if (sug?.target || sug?.suggest) return 'suggest-move'
  return 'suggest-none'
}

// 获取建议的目标文件夹名（不含文件名）
function getSuggestTarget(file: FileRecord): string {
  const sug = store.suggestions.get(file.id)
  if (!sug || sug.is_dup) return ''
  return sug.target || ''
}

// 当 action 变为 move/archive 时，自动填充 moveTarget
function handleActionChange(fileId: string, action: string) {
  store.setAction(fileId, action)
  if (action === 'move' || action === 'archive') {
    const file = store.files.find(f => f.id === fileId)
    if (file && !file.moveTarget) {
      const target = getSuggestTarget(file)
      if (target) store.setMoveTarget(fileId, target)
    }
  }
}

const batchAction = ref<string>('delete')

function handleBatchAction() {
  const ids = [...store.selectedIds]
  if (ids.length === 0) {
    message.warning('请先选择文件')
    return
  }
  store.batchSetAction(ids, batchAction.value)
  message.success(`已将 ${ids.length} 个文件设为${actionOptions.find(o => o.value === batchAction.value)?.label}`)
}

async function handleCleanup() {
  try {
    const result = await store.executeCleanup()
    if (!result) message.info('没有需要处理的操作')
    // Success message shown via WebSocket clean_complete
  } catch {
    message.error('清理失败')
  }
}

async function handleSearch() {
  store.setSearch(searchText.value)
}

function toggleSelect(id: string) {
  if (store.selectedIds.has(id)) {
    store.selectedIds.delete(id)
  } else {
    store.selectedIds.add(id)
  }
}

function toggleSelectAll() {
  if (store.selectedIds.size === filteredFiles.value.length) {
    store.selectedIds.clear()
  } else {
    store.selectedIds = new Set(filteredFiles.value.map(f => f.id))
  }
}

const filteredFiles = computed(() => {
  let list = store.files
  if (store.filterDup) {
    list = list.filter(f => {
      const sug = store.suggestions.get(f.id)
      return sug?.is_dup
    })
  }
  if (store.filterMultiVersion) {
    // Group files by base name (name before version), only show groups with 2+ versions
    const baseGroups = new Map<string, number>()
    const fileBases = new Map<string, string>()
    for (const f of store.files) {
      if (!f.version) continue
      // Extract base name: strip version-like segments
      const base = f.name.replace(/[-_\s]v?\d[\d.]*/i, '').replace(/\.[^.]+$/, '').toLowerCase()
      fileBases.set(f.id, base)
      baseGroups.set(base, (baseGroups.get(base) || 0) + 1)
    }
    list = list.filter(f => {
      const base = fileBases.get(f.id)
      return base !== undefined && (baseGroups.get(base) || 0) >= 2
    })
  }
  return list
})

const allSelected = computed(() => filteredFiles.value.length > 0 && filteredFiles.value.every(f => store.selectedIds.has(f.id)))
</script>

<template>
  <div class="file-list-view">
    <!-- Toolbar -->
    <div class="toolbar">
      <span class="toolbar-title">
        {{ store.filterDup ? '重复文件' : store.filterMultiVersion ? '多版本文件' : '全部文件' }} — 扫描结果
      </span>
      <div class="toolbar-actions">
        <button class="btn" @click="$router.push('/scan')">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><circle cx="6" cy="6" r="4.5" stroke="currentColor" stroke-width="1.2"/><path d="M6 4v4M4 6h4" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/></svg>
          选择目录
        </button>
        <button class="btn" @click="store.fetchFiles(); store.fetchStats(); store.fetchSuggestions()">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M1 6a5 5 0 1 0 10 0" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><path d="M6 1v5M4 4l2-3 2 3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
          重新扫描
        </button>
        <button class="btn primary" @click="handleCleanup">
          <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M2 3h8M5 3V2h2v1M3 3l.5 7h5L9 3" stroke="white" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
          执行清理
        </button>
      </div>
    </div>

    <!-- Scan bar -->
    <div class="scan-bar" v-if="scanStore.lastScanDirs || scanStore.scanComplete">
      <svg width="14" height="14" viewBox="0 0 14 14" fill="none"><circle cx="7" cy="7" r="5.5" stroke="#3B6D11" stroke-width="1.2"/><path d="M4.5 7l2 2 3-3" stroke="#3B6D11" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
      <p>已扫描完毕，发现 {{ store.stats.total }} 个文件，检测到 {{ store.stats.duplicates }} 处重复、{{ store.stats.multiversion }} 组多版本</p>
      <span class="path" v-if="scanStore.lastScanDirs">{{ scanStore.lastScanDirs }}</span>
    </div>

    <!-- Stats Cards -->
    <div class="stats-row">
      <div class="stat-card">
        <div class="label">总文件数</div>
        <div class="value">{{ store.stats.total }}</div>
        <div class="sub">{{ formatSize(store.stats.total_size || 0) }}</div>
      </div>
      <div class="stat-card">
        <div class="label">重复文件</div>
        <div class="value" style="color:#A32D2D">{{ store.stats.duplicates }}</div>
        <div class="sub">可释放空间</div>
      </div>
      <div class="stat-card">
        <div class="label">多版本</div>
        <div class="value" style="color:#185FA5">{{ store.stats.multiversion }}</div>
        <div class="sub">保留最新版本</div>
      </div>
      <div class="stat-card">
        <div class="label">待分类</div>
        <div class="value" style="color:#854F0B">{{ store.stats.uncategorized }}</div>
        <div class="sub">需人工确认</div>
      </div>
    </div>

    <!-- Batch action bar -->
    <div class="batch-bar" v-if="store.selectedIds.size > 0">
      <span class="batch-info">已选 {{ store.selectedIds.size }} 个文件</span>
      <div class="batch-actions">
        <n-select v-model:value="batchAction" :options="actionOptions" size="small" style="width: 100px" />
        <button class="btn small primary" @click="handleBatchAction">批量应用</button>
      </div>
    </div>

    <!-- Search -->
    <div class="search-bar">
      <input
        v-model="searchText"
        placeholder="搜索文件名..."
        class="search-input"
        @keyup.enter="handleSearch"
      />
    </div>

    <!-- Table -->
    <div class="table-area">
      <table class="file-table">
        <thead>
          <tr>
            <th style="width:30px"><input type="checkbox" :checked="allSelected" @change="toggleSelectAll" /></th>
            <th>文件名</th>
            <th>类型</th>
            <th>功能</th>
            <th>版本</th>
            <th>大小</th>
            <th>操作建议</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="file in filteredFiles" :key="file.id" :class="{ selected: store.selectedIds.has(file.id) }">
            <td><input type="checkbox" :checked="store.selectedIds.has(file.id)" @change="toggleSelect(file.id)" /></td>
            <td><span class="file-name">{{ file.name }}</span></td>
            <td>
              <span class="cat-tag" :style="getCatStyle(file.category)">{{ getCatLabel(file.category) }}</span>
            </td>
            <td>
              <span v-if="file.functionalCategory" class="func-tag">{{ file.functionalCategory }}</span>
              <span v-else class="empty-tag">—</span>
            </td>
            <td class="version-cell">{{ file.version || '—' }}</td>
            <td class="size-cell">{{ formatSize(file.size) }}</td>
            <td class="action-cell">
              <div class="suggest-row">
                <span v-if="getSuggestionText(file)" class="suggest-text" :class="getSuggestionClass(file)">
                  {{ getSuggestionText(file) }}
                </span>
                <span v-else class="suggest-text suggest-none">无建议</span>
                <select class="action-select" :value="file.action" @change="(e) => handleActionChange(file.id, (e.target as HTMLSelectElement).value)">
                  <option v-for="opt in actionOptions" :key="opt.value" :value="opt.value">{{ opt.label }}</option>
                </select>
              </div>
              <div v-if="file.action === 'move' || file.action === 'archive'" class="move-target-row">
                <input
                  class="move-target-input"
                  :value="file.moveTarget || getSuggestTarget(file)"
                  @input="(e) => store.setMoveTarget(file.id, (e.target as HTMLInputElement).value)"
                  placeholder="目标文件夹名"
                />
              </div>
            </td>
          </tr>
          <tr v-if="filteredFiles.length === 0 && !store.loading">
            <td colspan="7" class="empty-cell">暂无文件数据，请先扫描目录</td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Status bar -->
    <div class="status-bar">
      <div class="status-dot"></div>
      <span class="status-text">就绪 · {{ store.selectedIds.size > 0 ? `已选 ${store.selectedIds.size} 个文件待处理` : '未选择文件' }}</span>
    </div>

    <!-- Pagination -->
    <div class="pagination-bar" v-if="store.totalFiles > store.pageSize">
      <button class="btn small" :disabled="store.currentPage <= 1" @click="store.setPage(store.currentPage - 1)">上一页</button>
      <span class="page-info">{{ store.currentPage }} / {{ store.totalPages }}</span>
      <button class="btn small" :disabled="store.currentPage >= store.totalPages" @click="store.setPage(store.currentPage + 1)">下一页</button>
      <span class="total-info">共 {{ store.totalFiles }} 个文件</span>
    </div>
  </div>
</template>

<style scoped>
.file-list-view {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  background: #fff;
  padding: 10px 16px;
  border-radius: 8px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.toolbar-title {
  font-size: 14px;
  font-weight: 600;
  color: #1f2937;
}

.toolbar-actions {
  display: flex;
  gap: 6px;
}

.btn {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 5px 12px;
  border: 0.5px solid #d1d5db;
  border-radius: 6px;
  background: #fff;
  font-size: 12px;
  color: #374151;
  cursor: pointer;
  transition: all 0.15s;
}

.btn:hover { background: #f9fafb; }
.btn.danger { color: #A32D2D; border-color: #fca5a5; }
.btn.danger:hover { background: #fef2f2; }
.btn.primary { background: #185FA5; color: #fff; border-color: #185FA5; }
.btn.primary:hover { background: #144e8a; }
.btn.small { padding: 3px 8px; font-size: 11px; }
.btn:disabled { opacity: 0.5; cursor: not-allowed; }

.scan-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 10px 16px;
  background: #f0faf0;
  border-radius: 8px;
  font-size: 13px;
  color: #3B6D11;
}

.scan-bar p { margin: 0; }
.scan-bar .path { font-family: monospace; font-size: 11px; color: #6b7280; }

.stats-row {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 10px;
}

.stat-card {
  background: #fff;
  border-radius: 8px;
  padding: 14px 18px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.stat-card .label {
  font-size: 11px;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.stat-card .value {
  font-size: 24px;
  font-weight: 700;
  color: #1f2937;
  line-height: 1.3;
}

.stat-card .sub {
  font-size: 11px;
  color: #9ca3af;
  margin-top: 2px;
}

.batch-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 16px;
  background: #EFF6FF;
  border-radius: 8px;
}

.batch-info { font-size: 13px; color: #185FA5; font-weight: 500; }
.batch-actions { display: flex; gap: 8px; align-items: center; }

.search-bar { display: flex; }

.search-input {
  flex: 1;
  padding: 8px 14px;
  border: 0.5px solid #e5e7eb;
  border-radius: 6px;
  font-size: 13px;
  outline: none;
  background: #fff;
}

.search-input:focus { border-color: #185FA5; box-shadow: 0 0 0 2px rgba(24,95,165,0.1); }

.table-area {
  background: #fff;
  border-radius: 8px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
  overflow: hidden;
}

.file-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 13px;
}

.file-table th {
  text-align: left;
  padding: 8px 12px;
  font-size: 11px;
  font-weight: 600;
  color: #9ca3af;
  text-transform: uppercase;
  background: #fafafa;
  border-bottom: 0.5px solid #e5e7eb;
}

.file-table td {
  padding: 8px 12px;
  border-bottom: 0.5px solid #f3f4f6;
}

.file-table tr.selected { background: #EFF6FF; }
.file-table tr:hover { background: #f9fafb; }

.cat-tag {
  white-space: nowrap;
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

.file-name {
  font-family: monospace;
  font-size: 11px;
  color: #1f2937;
}

.version-cell { color: #6b7280; }
.size-cell { color: #6b7280; }

.action-cell { min-width: 200px; }

.suggest-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.suggest-text {
  font-size: 11px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 140px;
}

.suggest-text.suggest-delete { color: #A32D2D; }
.suggest-text.suggest-move { color: #3B6D11; }
.suggest-text.suggest-none { color: #9ca3af; }

.action-select {
  padding: 3px 6px;
  border: 0.5px solid #d1d5db;
  border-radius: 4px;
  font-size: 11px;
  background: #fff;
  cursor: pointer;
  outline: none;
  flex-shrink: 0;
}

.action-select:focus { border-color: #185FA5; }

.move-target-row { margin-top: 4px; }
.move-target-input {
  width: 100%;
  padding: 3px 6px;
  border: 0.5px solid #d1d5db;
  border-radius: 4px;
  font-size: 11px;
  outline: none;
  box-sizing: border-box;
}
.move-target-input:focus { border-color: #185FA5; }
.move-target-input::placeholder { color: #9ca3af; }

.empty-cell {
  text-align: center;
  padding: 40px 20px !important;
  color: #9ca3af;
  font-size: 13px;
}

.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  background: #fff;
  border-radius: 6px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #3B6D11;
}

.status-text {
  font-size: 11px;
  color: #6b7280;
}

.pagination-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
}

.page-info { font-size: 12px; color: #6b7280; }
.total-info { font-size: 12px; color: #9ca3af; margin-left: auto; }
</style>
