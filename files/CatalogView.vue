<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useMessage } from 'naive-ui'
import axios from 'axios'
import Papa from 'papaparse'

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
      homepageUrl: e.homepageUrl ?? e.homepage_url ?? '',
      downloadUrl: e.downloadUrl ?? e.download_url ?? '',
      latestVersion: e.latestVersion ?? e.latest_version ?? '',
      functionalCategory: e.functionalCategory ?? e.functional_category ?? '',
      aiConfidence: e.aiConfidence ?? e.ai_confidence ?? 0,
      aiProvider: e.aiProvider ?? e.ai_provider ?? '',
      metaUpdatedAt: e.metaUpdatedAt ?? e.meta_updated_at ?? '',
      needsReview: e.needsReview ?? e.needs_review ?? false,
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

function formatDate(dateStr: string): string {
  if (!dateStr || dateStr === '0001-01-01T00:00:00Z') return '-'
  return new Date(dateStr).toLocaleDateString('zh-CN')
}

function handleSearch() {
  fetchCatalog()
}

// ── Export CSV ──────────────────────────────────────────────
function exportCSV() {
  if (entries.value.length === 0) {
    message.warning('无数据可导出')
    return
  }
  const data = entries.value.map(e => ({
    名称: e.name,
    功能分类: e.functionalCategory || '',
    描述: e.description || '',
    最新版本: e.latestVersion || '',
    官网: e.homepageUrl || '',
    下载链接: e.downloadUrl || '',
    许可证: e.license || '',
    标签: Array.isArray(e.tags) ? e.tags.join(';') : '',
    置信度: e.aiConfidence ? (e.aiConfidence * 100).toFixed(0) + '%' : '',
    AI提供者: e.aiProvider || '',
    更新时间: formatDate(e.metaUpdatedAt),
    待审核: e.needsReview ? '是' : '否',
  }))
  const csv = Papa.unparse(data)
  downloadFile('﻿' + csv, `FileSweep_目录_${today()}.csv`, 'text/csv;charset=utf-8')
  message.success('CSV 导出成功')
  exportMenuVisible.value = false
}

// ── Export Obsidian Markdown ──────────────────────────────
function exportObsidian() {
  if (entries.value.length === 0) {
    message.warning('无数据可导出')
    return
  }

  const lines: string[] = []
  lines.push(`---`)
  lines.push(`title: FileSweep 软件目录`)
  lines.push(`date: ${new Date().toISOString().slice(0, 10)}`)
  lines.push(`tags: [软件目录, FileSweep]`)
  lines.push(`---`)
  lines.push(``)
  lines.push(`# 软件目录`)
  lines.push(``)
  lines.push(`> 导出于 ${new Date().toLocaleString('zh-CN')}，共 ${entries.value.length} 条记录`)
  lines.push(``)

  // Group by functional category
  const groups = new Map<string, CatalogEntry[]>()
  for (const e of entries.value) {
    const cat = e.functionalCategory || '未分类'
    if (!groups.has(cat)) groups.set(cat, [])
    groups.get(cat)!.push(e)
  }

  // Sort groups alphabetically
  const sortedGroups = [...groups.entries()].sort((a, b) => a[0].localeCompare(b[0]))

  for (const [cat, items] of sortedGroups) {
    lines.push(`## ${cat}`)
    lines.push(``)

    for (const e of items) {
      lines.push(`### ${e.name}`)
      lines.push(``)

      if (e.description) {
        lines.push(e.description)
        lines.push(``)
      }

      lines.push(`| 字段 | 值 |`)
      lines.push(`| --- | --- |`)
      if (e.latestVersion) lines.push(`| 最新版本 | \`${e.latestVersion}\` |`)
      if (e.license) lines.push(`| 许可证 | ${e.license} |`)
      if (e.homepageUrl) lines.push(`| 官网 | [${e.homepageUrl}](${e.homepageUrl}) |`)
      if (e.downloadUrl) lines.push(`| 下载 | [下载页面](${e.downloadUrl}) |`)
      if (e.aiProvider) lines.push(`| AI 来源 | ${e.aiProvider} (${e.aiConfidence ? (e.aiConfidence * 100).toFixed(0) + '%' : 'N/A'}) |`)
      lines.push(``)

      if (e.tags && e.tags.length > 0) {
        lines.push(`**标签**: ${e.tags.map(t => `#${t.replace(/\s/g, '_')}`).join(' ')}`)
        lines.push(``)
      }

      lines.push(`---`)
      lines.push(``)
    }
  }

  downloadFile(lines.join('\n'), `FileSweep_目录_${today()}.md`, 'text/markdown;charset=utf-8')
  message.success('Obsidian Markdown 导出成功')
  exportMenuVisible.value = false
}

function downloadFile(content: string, filename: string, mime: string) {
  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

function today() {
  return new Date().toISOString().slice(0, 10)
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
        <!-- Export dropdown -->
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
            <button class="export-item" @click="exportCSV">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                <rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 21V9"/>
              </svg>
              导出 CSV
            </button>
            <button class="export-item" @click="exportObsidian">
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
              <button class="btn-delete" @click="deleteEntry(entry.id)" title="删除">
                <svg width="12" height="12" viewBox="0 0 12 12" fill="none"><path d="M2 3h8M5 3V2h2v1M3 3l.5 7h5L9 3" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/></svg>
              </button>
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

.btn-delete {
  background: none; border: none; cursor: pointer; color: #d1d5db;
  padding: 4px; border-radius: 4px; display: flex; align-items: center;
}
.btn-delete:hover { background: #fee2e2; color: #A32D2D; }

.empty-cell { text-align: center; padding: 40px 20px !important; color: #9ca3af; }
</style>
