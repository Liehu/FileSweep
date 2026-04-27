import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import axios from 'axios'

export interface CatalogEntry {
  id: string
  name: string
  vendor: string
  latest_version: string
  category: string
  description: string
  homepage_url: string
  download_url: string
  file_count: number
  updated_at: string
}

export const useCatalogStore = defineStore('catalog', () => {
  const entries = ref<CatalogEntry[]>([])
  const loading = ref(false)
  const error = ref<string | null>(null)
  const searchQuery = ref('')
  const viewMode = ref<'card' | 'table'>('card')

  const filteredEntries = computed(() => {
    if (!searchQuery.value) return entries.value
    const q = searchQuery.value.toLowerCase()
    return entries.value.filter(
      (e) =>
        e.name.toLowerCase().includes(q) ||
        e.vendor.toLowerCase().includes(q) ||
        e.category.toLowerCase().includes(q)
    )
  })

  async function fetchCatalog() {
    loading.value = true
    error.value = null
    try {
      const resp = await axios.get('/api/catalog')
      const body = resp.data
      entries.value = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
    } catch (e: unknown) {
      error.value = (e as Error).message || '获取软件目录失败'
    } finally {
      loading.value = false
    }
  }

  async function searchCatalog(query: string) {
    searchQuery.value = query
    if (!query) {
      await fetchCatalog()
      return
    }
    loading.value = true
    try {
      const resp = await axios.get('/api/catalog', { params: { search: query } })
      const body = resp.data
      entries.value = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
    } catch (e: unknown) {
      error.value = (e as Error).message || '搜索失败'
    } finally {
      loading.value = false
    }
  }

  async function getEntry(id: string): Promise<CatalogEntry | null> {
    try {
      const resp = await axios.get(`/api/catalog/${id}`)
      return resp.data
    } catch {
      return null
    }
  }

  function setViewMode(mode: 'card' | 'table') {
    viewMode.value = mode
  }

  return {
    entries,
    loading,
    error,
    searchQuery,
    viewMode,
    filteredEntries,
    fetchCatalog,
    searchCatalog,
    getEntry,
    setViewMode,
  }
})
