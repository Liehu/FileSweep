import { defineStore } from "pinia";
import { ref } from "vue";
import { invoke } from "@/lib/api";

export interface CatalogEntry {
  id: string;
  name: string;
  description: string;
  homepageUrl: string;
  downloadUrl: string;
  latestVersion: string;
  license: string;
  functionalCategory: string;
  tags: string[];
  aiConfidence: number;
  aiProvider: string;
  metaUpdatedAt: string;
  notes: string;
  needsReview: boolean;
  aiSkip: boolean;
}

export const useCatalogStore = defineStore("catalog", () => {
  const entries = ref<CatalogEntry[]>([]);
  const loading = ref(false);
  const error = ref<string | null>(null);
  const searchQuery = ref("");
  const viewMode = ref<"card" | "table">("table");

  const page = ref(1);
  const pageSize = ref(20);
  const total = ref(0);
  const totalPages = ref(0);

  async function fetchCatalog() {
    loading.value = true;
    error.value = null;
    try {
      const params: Record<string, any> = { page: page.value, pageSize: pageSize.value };
      if (searchQuery.value) params.search = searchQuery.value;
      const res = await invoke<any>("get_catalog", params);
      entries.value = res.entries || res.items || [];
      total.value = res.total || 0;
      totalPages.value = Math.ceil(total.value / pageSize.value);
    } catch (e) {
      error.value = String(e);
    } finally {
      loading.value = false;
    }
  }

  async function updateEntry(id: string, data: Partial<CatalogEntry>) {
    try {
      await invoke("update_catalog_entry", { id, ...data });
      await fetchCatalog();
    } catch (e) {
      error.value = String(e);
      throw e;
    }
  }

  async function deleteEntry(id: string) {
    try {
      await invoke("delete_catalog_entry", { id });
      await fetchCatalog();
    } catch (e) {
      error.value = String(e);
      throw e;
    }
  }

  async function exportCsv(): Promise<string> {
    try {
      return await invoke<string>("export_catalog", { format: "csv" });
    } catch (e) {
      error.value = String(e);
      throw e;
    }
  }

  async function exportObsidianMd(): Promise<string> {
    try {
      return await invoke<string>("export_catalog", { format: "obsidian" });
    } catch (e) {
      error.value = String(e);
      throw e;
    }
  }

  return {
    entries, loading, error, searchQuery, viewMode,
    page, pageSize, total, totalPages,
    fetchCatalog, updateEntry, deleteEntry, exportCsv, exportObsidianMd,
  };
});
