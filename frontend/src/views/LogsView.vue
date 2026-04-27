<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import {
  NButton,
  NInput,
  NSelect,
  NEmpty,
  NSpin,
  NCheckbox,
  useMessage,
} from 'naive-ui'
import axios from 'axios'
import Papa from 'papaparse'

interface LogEntry {
  id: string
  timestamp: string
  action: string
  target: string
  details: string
  status: string
  user: string
  canRevert: boolean
  checked: boolean
}

const message = useMessage()

const logs = ref<LogEntry[]>([])
const loading = ref(false)
const filterAction = ref<string | null>(null)
const filterStatus = ref<string | null>(null)
const filterSearch = ref('')

const actionOptions = [
  { label: '全部', value: '' },
  { label: '扫描', value: 'scan' },
  { label: '删除', value: 'delete' },
  { label: '清理', value: 'cleanup' },
  { label: '整理', value: 'organize' },
  { label: '丰富', value: 'enrich' },
]

const statusOptions = [
  { label: '全部', value: '' },
  { label: '成功', value: 'success' },
  { label: '失败', value: 'error' },
  { label: '警告', value: 'warning' },
  { label: '已回退', value: 'reverted' },
]

onMounted(() => {
  fetchLogs()
})

async function fetchLogs() {
  loading.value = true
  try {
    const params: Record<string, unknown> = {}
    if (filterAction.value) params.action = filterAction.value
    if (filterStatus.value) params.status = filterStatus.value
    if (filterSearch.value) params.q = filterSearch.value
    const resp = await axios.get('/api/logs', { params })
    const body = resp.data
    const items = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
    logs.value = items.map((l: Record<string, unknown>) => ({
      id: String(l.id ?? Math.random()),
      timestamp: l.timestamp ?? '',
      action: l.operation ?? l.action ?? '',
      target: (l.source_path ?? l.target ?? '') as string,
      details: buildDetails(l),
      status: (l.status ?? 'success') as string,
      user: '',
      canRevert: (l.can_revert ?? l.canRevert ?? false) as boolean,
      checked: false,
    }))
  } catch {
    message.error('获取日志失败')
  } finally {
    loading.value = false
  }
}

function buildDetails(l: Record<string, unknown>): string {
  const op = (l.operation ?? l.action ?? '') as string
  const source = (l.sourcePath ?? l.source_path ?? '') as string
  const dest = (l.destPath ?? l.dest_path ?? '') as string
  const reason = (l.reason ?? l.details ?? '') as string
  // Extract filename from full path for display
  const fileName = source ? source.split(/[/\\]/).pop() || source : '-'
  const destName = dest ? dest.split(/[/\\]/).pop() || dest : ''

  const parts: string[] = []
  switch (op.toUpperCase()) {
    case 'DELETE':
      parts.push(`删除: ${fileName}`)
      break
    case 'MOVE':
      parts.push(`移动: ${fileName} → ${destName || dest || '-'}`)
      break
    case 'SCAN':
      parts.push(`扫描目录: ${source || '-'}`)
      break
    default:
      if (source) parts.push(`${fileName}`)
      if (dest) parts.push(`→ ${destName || dest}`)
  }
  if (reason) parts.push(`(${reason})`)
  return parts.join(' ')
}

function handleFilter() {
  fetchLogs()
}

function handleReset() {
  filterAction.value = null
  filterStatus.value = null
  filterSearch.value = ''
  fetchLogs()
}

function getStatusColor(status: string): string {
  const map: Record<string, string> = {
    success: '#3B6D11',
    error: '#A32D2D',
    warning: '#854F0B',
    reverted: '#6B7280',
    dry_run: '#185FA5',
  }
  return map[status] || '#6B7280'
}

function getStatusBg(status: string): string {
  const map: Record<string, string> = {
    success: '#EAF3DE',
    error: '#FCEBEB',
    warning: '#FAEEDA',
    reverted: '#F3F4F6',
    dry_run: '#E6F1FB',
  }
  return map[status] || '#F3F4F6'
}

function getStatusLabel(status: string): string {
  const map: Record<string, string> = {
    success: '成功',
    error: '失败',
    warning: '警告',
    reverted: '已回退',
    dry_run: '预览',
  }
  return map[status] || status
}

function formatTime(ts: string): string {
  if (!ts) return '-'
  return new Date(ts).toLocaleString('zh-CN')
}

// Selection
const checkedRevertable = computed(() =>
  logs.value.filter(l => l.checked && l.canRevert)
)
const allRevertableChecked = computed(() => {
  const revertable = logs.value.filter(l => l.canRevert && l.status !== 'reverted')
  return revertable.length > 0 && revertable.every(l => l.checked)
})

function toggleAll(val: boolean) {
  logs.value.forEach(l => {
    if (l.canRevert && l.status !== 'reverted') l.checked = val
  })
}

async function revertLog(logEntry: LogEntry) {
  try {
    await axios.post(`/api/logs/${logEntry.id}/revert`)
    message.success('操作已回退')
    fetchLogs()
  } catch (err: any) {
    const msg = getRevertErrorMsg(err?.response?.data?.error || err?.message || '')
    message.error(msg)
  }
}

async function batchRevert() {
  const ids = checkedRevertable.value.map(l => Number(l.id))
  if (ids.length === 0) return

  try {
    const resp = await axios.post('/api/logs/batch-revert', { ids })
    const results = resp.data.results ?? []
    const succeeded = results.filter((r: { ok: boolean }) => r.ok).length
    const failed = results.filter((r: { ok: boolean }) => !r.ok)

    if (failed.length === 0) {
      message.success(`批量回退成功，共 ${succeeded} 条`)
    } else {
      const msgs = failed.map((r: { id: number; error: string }) => `#${r.id}: ${getRevertErrorMsg(r.error)}`)
      message.warning(`成功 ${succeeded} 条，失败 ${failed.length} 条: ${msgs.join('; ')}`)
    }
    fetchLogs()
  } catch {
    message.error('批量回退请求失败')
  }
}

function getRevertErrorMsg(raw: string): string {
  if (raw.includes('回收站中未找到') || raw.includes('not found')) {
    return '回收站已清空或文件已被改名，无法回退'
  }
  if (raw.includes('不可回退')) return '该操作不可回退'
  return raw || '回退失败'
}

function exportCSV() {
  if (logs.value.length === 0) {
    message.warning('无日志数据可导出')
    return
  }
  const data = logs.value.map((log) => ({
    时间: formatTime(log.timestamp),
    操作: log.action,
    详情: log.details,
    状态: getStatusLabel(log.status),
  }))
  const csv = Papa.unparse(data)
  const blob = new Blob(['﻿' + csv], { type: 'text/csv;charset=utf-8' })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = `FileSweep_日志_${new Date().toISOString().slice(0, 10)}.csv`
  a.click()
  URL.revokeObjectURL(url)
  message.success('导出成功')
}
</script>

<template>
  <div class="logs-view">
    <h2 class="page-title">操作日志</h2>

    <!-- Filter Bar -->
    <div class="filter-bar">
      <div class="filter-left">
        <n-input
          v-model:value="filterSearch"
          placeholder="搜索日志..."
          clearable
          style="width: 200px"
          @keyup.enter="handleFilter"
          @clear="handleFilter"
        />
        <n-select
          v-model:value="filterAction"
          :options="actionOptions"
          placeholder="操作类型"
          clearable
          style="width: 120px"
          @update:value="handleFilter"
        />
        <n-select
          v-model:value="filterStatus"
          :options="statusOptions"
          placeholder="状态"
          clearable
          style="width: 100px"
          @update:value="handleFilter"
        />
        <n-button @click="handleReset">重置</n-button>
      </div>
      <div class="filter-right">
        <n-button
          type="warning"
          :disabled="checkedRevertable.length === 0"
          @click="batchRevert"
        >
          批量回退 ({{ checkedRevertable.length }})
        </n-button>
        <n-button @click="exportCSV">
          <template #icon>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
              <polyline points="7 10 12 15 17 10" /><line x1="12" y1="15" x2="12" y2="3" />
            </svg>
          </template>
          导出 CSV
        </n-button>
      </div>
    </div>

    <!-- Table -->
    <div class="table-card">
      <n-spin :show="loading">
        <table class="logs-table">
          <thead>
            <tr>
              <th style="width:36px">
                <n-checkbox
                  :checked="allRevertableChecked"
                  @update:checked="toggleAll"
                  :indeterminate="checkedRevertable.length > 0 && !allRevertableChecked"
                />
              </th>
              <th style="width:160px">时间</th>
              <th>操作详情</th>
              <th style="width:80px">状态</th>
              <th style="width:70px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="log in logs" :key="log.id">
              <td>
                <n-checkbox
                  v-if="log.canRevert && log.status !== 'reverted'"
                  v-model:checked="log.checked"
                />
              </td>
              <td>
                <span class="time-text">{{ formatTime(log.timestamp) }}</span>
              </td>
              <td class="detail-cell">
                <span class="action-tag">{{ log.action }}</span>
                <span class="detail-text">{{ log.details }}</span>
              </td>
              <td>
                <span
                  class="status-tag"
                  :style="{ background: getStatusBg(log.status), color: getStatusColor(log.status) }"
                >
                  {{ getStatusLabel(log.status) }}
                </span>
              </td>
              <td>
                <button
                  v-if="log.canRevert && log.status !== 'reverted'"
                  class="btn-revert"
                  @click="revertLog(log)"
                >回退</button>
                <span v-else-if="log.status === 'reverted'" class="reverted-text">-</span>
              </td>
            </tr>
            <tr v-if="logs.length === 0 && !loading">
              <td colspan="5" class="empty-cell">暂无日志记录</td>
            </tr>
          </tbody>
        </table>
        <n-empty v-if="!loading && logs.length === 0" description="暂无日志记录" style="padding: 40px 0" />
      </n-spin>
    </div>
  </div>
</template>

<style scoped>
.logs-view { display: flex; flex-direction: column; gap: 16px; }

.page-title { font-size: 20px; font-weight: 700; color: #1f2937; margin: 0; }

.filter-bar {
  display: flex; align-items: center; justify-content: space-between;
  background: #fff; padding: 12px 16px; border-radius: 8px;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05);
}

.filter-left { display: flex; gap: 8px; align-items: center; }
.filter-right { display: flex; gap: 8px; align-items: center; }

.table-card {
  background: #fff; border-radius: 8px; padding: 4px 0;
  box-shadow: 0 1px 2px rgba(0, 0, 0, 0.05); overflow: hidden;
}

.logs-table { width: 100%; border-collapse: collapse; font-size: 13px; }
.logs-table th {
  text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600;
  color: #9ca3af; text-transform: uppercase; background: #fafafa;
  border-bottom: 0.5px solid #e5e7eb;
}
.logs-table td { padding: 8px 12px; border-bottom: 0.5px solid #f3f4f6; }
.logs-table tr:hover { background: #f9fafb; }

.time-text { font-size: 13px; color: #6b7280; font-variant-numeric: tabular-nums; }

.detail-cell { max-width: 600px; }
.action-tag {
  display: inline-block; padding: 2px 8px; border-radius: 4px;
  font-size: 12px; font-weight: 500; background: #E6F1FB; color: #185FA5;
  margin-right: 8px; white-space: nowrap;
}
.detail-text { font-size: 13px; color: #4b5563; }

.status-tag {
  display: inline-block; padding: 2px 8px; border-radius: 4px;
  font-size: 12px; font-weight: 600;
}

.btn-revert {
  padding: 2px 10px; border: 1px solid #fbbf24; border-radius: 4px;
  background: #fffbeb; color: #92400e; font-size: 12px; cursor: pointer;
}
.btn-revert:hover { background: #fef3c7; }
.reverted-text { color: #d1d5db; }

.empty-cell { text-align: center; padding: 40px 20px !important; color: #9ca3af; }
</style>
