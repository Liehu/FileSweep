import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import axios from 'axios'

export interface FileRecord {
  id: string
  name: string
  path: string
  size: number
  category: string
  functionalCategory: string
  version: string
  hash: string
  extension: string
  action: string
  moveTarget: string
  scanned_at: string
}

export interface FileSuggestion {
  id: string
  is_dup: boolean
  reason: string
  target: string
  suggest: string
}

export interface FileStats {
  total: number
  duplicates: number
  multiversion: number
  uncategorized: number
  total_size: number
}

export const useFilesStore = defineStore('files', () => {
  const files = ref<FileRecord[]>([])
  const stats = ref<FileStats>({ total: 0, duplicates: 0, multiversion: 0, uncategorized: 0, total_size: 0 })
  const loading = ref(false)
  const error = ref<string | null>(null)
  const currentPage = ref(1)
  const pageSize = ref(50)
  const totalFiles = ref(0)
  const selectedIds = ref<Set<string>>(new Set())
  const filterCategory = ref<string>('')
  const searchQuery = ref('')
  const filterDup = ref(false)
  const filterMultiVersion = ref(false)
  const suggestions = ref<Map<string, FileSuggestion>>(new Map())
  const lastScanDir = ref('')

  const totalPages = computed(() => Math.ceil(totalFiles.value / pageSize.value))

  function mapRecord(r: Record<string, unknown>): FileRecord {
    return {
      id: (r.id ?? '') as string,
      name: (r.name ?? '') as string,
      path: (r.localPath ?? r.path ?? '') as string,
      size: (r.fileSize ?? r.size ?? 0) as number,
      category: (r.category ?? '') as string,
      functionalCategory: (r.functional_category ?? r.functionalCategory ?? '') as string,
      version: (r.version ?? '') as string,
      hash: (r.fileHash ?? r.hash ?? '') as string,
      extension: (r.extension ?? '') as string,
      action: (r.action ?? 'keep') as string,
      moveTarget: '',
      scanned_at: (r.scannedAt ?? r.scanned_at ?? '') as string,
    }
  }

  async function fetchFiles() {
    loading.value = true
    error.value = null
    try {
      const params: Record<string, unknown> = {
        page: currentPage.value,
        page_size: pageSize.value,
      }
      if (filterCategory.value) params.category = filterCategory.value
      if (searchQuery.value) params.search = searchQuery.value

      const resp = await axios.get('/api/files', { params })
      const body = resp.data
      const raw: unknown[] = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
      files.value = raw.map((v: unknown) => mapRecord(v as Record<string, unknown>))
      totalFiles.value = body.total ?? files.value.length
    } catch (e: unknown) {
      error.value = (e as Error).message || '获取文件列表失败'
    } finally {
      loading.value = false
    }
  }

  async function fetchStats() {
    try {
      const resp = await axios.get('/api/files/stats')
      stats.value = resp.data
    } catch {
      stats.value = { total: 0, duplicates: 0, multiversion: 0, uncategorized: 0, total_size: 0 }
    }
  }

  async function fetchSuggestions() {
    try {
      const resp = await axios.get('/api/files/suggestions')
      const raw: FileSuggestion[] = resp.data.data ?? resp.data
      const map = new Map<string, FileSuggestion>()
      for (const s of raw) {
        map.set(s.id, s)
      }
      suggestions.value = map
      // Auto-apply suggestions to file actions (only for files still at default 'keep')
      for (const f of files.value) {
        const sug = map.get(f.id)
        if (!sug || f.action !== 'keep') continue
        if (sug.is_dup) {
          f.action = 'delete'
        } else if (sug.target) {
          f.action = 'move'
          if (!f.moveTarget) f.moveTarget = sug.target
        }
      }
    } catch {
      suggestions.value = new Map()
    }
  }

  async function startScan(dirs: string[], recursive = true, excludeDirs: string[] = [], excludeNames: string[] = [], excludeExts: string[] = []) {
    const resp = await axios.post('/api/scan', { dirs, recursive, exclude_dirs: excludeDirs, exclude_names: excludeNames, exclude_exts: excludeExts })
    lastScanDir.value = dirs.join('; ')
    return resp.data
  }

  async function executeCleanup() {
    const fileActions = files.value
      .filter(f => f.action && f.action !== 'keep')
      .map(f => ({ id: f.id, action: f.action, target: f.moveTarget || '' }))
    if (fileActions.length === 0) return null
    const resp = await axios.post('/api/clean', { confirm: true, file_actions: fileActions })
    // Data refresh is handled by WebSocket clean_complete event in ScanView
    return resp.data
  }

  async function applyOrganize() {
    return executeCleanup()
  }

  function setPage(page: number) {
    currentPage.value = page
    fetchFiles()
  }

  function setFilterCategory(cat: string) {
    filterCategory.value = cat
    currentPage.value = 1
    fetchFiles()
  }

  function setFilterDup(val: boolean) {
    filterDup.value = val
    if (val) filterMultiVersion.value = false
  }

  function setFilterMultiVersion(val: boolean) {
    filterMultiVersion.value = val
    if (val) filterDup.value = false
  }

  function setSearch(q: string) {
    searchQuery.value = q
    currentPage.value = 1
    fetchFiles()
  }

  function setAction(id: string, action: string) {
    const f = files.value.find((f) => f.id === id)
    if (f) {
      f.action = action
      // 切换到非移动操作时清空目标
      if (action !== 'move' && action !== 'archive') f.moveTarget = ''
    }
  }

  function setMoveTarget(id: string, target: string) {
    const f = files.value.find((f) => f.id === id)
    if (f) f.moveTarget = target
  }

  function batchSetAction(ids: string[], action: string) {
    const idSet = new Set(ids)
    for (const f of files.value) {
      if (idSet.has(f.id)) f.action = action
    }
  }

  return {
    files, stats, loading, error, currentPage, pageSize, totalFiles, totalPages,
    selectedIds, filterCategory, searchQuery, lastScanDir, suggestions,
    filterDup, filterMultiVersion,
    fetchFiles, fetchStats, fetchSuggestions, startScan, executeCleanup, applyOrganize,
    setPage, setFilterCategory, setFilterDup, setFilterMultiVersion, setSearch, setAction, setMoveTarget, batchSetAction,
  }
})
