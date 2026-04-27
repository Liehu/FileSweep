import { defineStore } from 'pinia'
import { ref, reactive } from 'vue'

export const useScanStore = defineStore('scan', () => {
  // Config — persisted across tab switches
  const dirs = ref<string[]>([''])
  const exclusions = reactive({
    dirs: '',
    names: '',
    exts: '',
  })

  // Progress — survives navigation
  const scanning = ref(false)
  const progress = ref(0)
  const statusText = ref('就绪')
  const totalFiles = ref(0)
  const currentFile = ref('')
  const elapsed = ref(0)
  const lastScanDirs = ref('')
  const scanComplete = ref(false)

  function addDir() {
    dirs.value.push('')
  }

  function removeDir(index: number) {
    if (dirs.value.length > 1) {
      dirs.value.splice(index, 1)
    }
  }

  function resetProgress() {
    scanning.value = false
    progress.value = 0
    statusText.value = '就绪'
    totalFiles.value = 0
    currentFile.value = ''
    elapsed.value = 0
    scanComplete.value = false
  }

  return {
    dirs, exclusions,
    scanning, progress, statusText, totalFiles, currentFile, elapsed, lastScanDirs, scanComplete,
    addDir, removeDir, resetProgress,
  }
})
