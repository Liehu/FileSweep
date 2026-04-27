<script setup lang="ts">
import { onMounted, onUnmounted } from 'vue'
import { useMessage } from 'naive-ui'
import { useScanStore } from '../stores/scan'
import { useFilesStore } from '../stores/files'

const message = useMessage()
const scanStore = useScanStore()
const fileStore = useFilesStore()

let ws: WebSocket | null = null
let elapsedTimer: ReturnType<typeof setInterval> | null = null

onMounted(() => {
  connectWebSocket()
  if (scanStore.scanning && !elapsedTimer) {
    startElapsedTimer()
  }
})

onUnmounted(() => {
  if (ws) {
    ws.close()
    ws = null
  }
  if (elapsedTimer) {
    clearInterval(elapsedTimer)
    elapsedTimer = null
  }
})

function connectWebSocket() {
  if (ws) return
  const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
  ws = new WebSocket(`${protocol}//${window.location.host}/ws`)

  ws.onmessage = (event) => {
    try {
      const data = JSON.parse(event.data)
      if (data.type === 'scan_progress') {
        const p = data.payload ?? data
        scanStore.progress = p.percent ?? 0
        scanStore.totalFiles = p.total ?? 0
        scanStore.currentFile = p.currentFile ?? p.current_file ?? ''
        if (scanStore.progress >= 100) {
          finishScan()
        }
      } else if (data.type === 'scan_complete') {
        scanStore.totalFiles = data.payload?.total ?? data.total ?? scanStore.totalFiles
        finishScan()
        message.success(`扫描完成，发现 ${scanStore.totalFiles} 个文件`)
      } else if (data.type === 'scan_error') {
        scanStore.scanning = false
        scanStore.statusText = '扫描失败'
        stopElapsedTimer()
        message.error('扫描失败')
      }
    } catch { /* ignore */ }
  }

  ws.onclose = () => {
    ws = null
    if (scanStore.scanning) {
      setTimeout(connectWebSocket, 3000)
    }
  }
  ws.onerror = () => { /* ws will close */ }
}

function finishScan() {
  scanStore.scanning = false
  scanStore.progress = 100
  scanStore.statusText = '扫描完成'
  scanStore.scanComplete = true
  stopElapsedTimer()
  fileStore.fetchStats()
  fileStore.fetchFiles()
}

function startElapsedTimer() {
  if (elapsedTimer) clearInterval(elapsedTimer)
  elapsedTimer = setInterval(() => { scanStore.elapsed++ }, 1000)
}

function stopElapsedTimer() {
  if (elapsedTimer) { clearInterval(elapsedTimer); elapsedTimer = null }
}

async function startScan() {
  const validDirs = scanStore.dirs.map(d => d.trim()).filter(Boolean)
  if (validDirs.length === 0) {
    message.warning('请输入至少一个扫描目录路径')
    return
  }

  const excludeDirs = scanStore.exclusions.dirs.split(/[,;，；\n]/).map(s => s.trim()).filter(Boolean)
  const excludeNames = scanStore.exclusions.names.split(/[,;，；\n]/).map(s => s.trim()).filter(Boolean)
  const excludeExts = scanStore.exclusions.exts.split(/[,;，；\n]/).map(s => s.trim()).filter(Boolean)

  scanStore.scanning = true
  scanStore.progress = 0
  scanStore.statusText = '扫描中...'
  scanStore.elapsed = 0
  scanStore.scanComplete = false
  scanStore.lastScanDirs = validDirs.join('; ')
  startElapsedTimer()

  try {
    await fileStore.startScan(validDirs, true, excludeDirs, excludeNames, excludeExts)
    message.info('扫描已启动')
  } catch {
    scanStore.scanning = false
    scanStore.statusText = '扫描失败'
    stopElapsedTimer()
    message.error('启动扫描失败')
  }
}

function formatElapsed(seconds: number): string {
  const m = Math.floor(seconds / 60)
  const s = seconds % 60
  return `${m.toString().padStart(2, '0')}:${s.toString().padStart(2, '0')}`
}

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}
</script>

<template>
  <div class="scan-view">
    <h2 class="page-title">文件扫描</h2>

    <!-- Directory inputs -->
    <div class="scan-section">
      <div class="section-label">扫描目录（支持多路径）</div>
      <div class="dir-list">
        <div v-for="(_, index) in scanStore.dirs" :key="index" class="dir-row">
          <input
            v-model="scanStore.dirs[index]"
            placeholder="输入目录路径，例如: D:\Downloads"
            class="dir-input"
            :disabled="scanStore.scanning"
            @keyup.enter="startScan"
          />
          <button v-if="scanStore.dirs.length > 1" class="btn-icon remove" @click="scanStore.removeDir(index)" title="移除此路径">×</button>
        </div>
        <button class="btn-add" @click="scanStore.addDir" :disabled="scanStore.scanning">+ 添加目录</button>
      </div>
    </div>

    <!-- Exclusions -->
    <div class="scan-section">
      <div class="section-label">排除规则（可选）</div>
      <div class="exclusion-grid">
        <div class="excl-item">
          <label class="excl-label">排除目录</label>
          <textarea v-model="scanStore.exclusions.dirs" class="excl-input" placeholder="如: node_modules, .git, __pycache__&#10;用逗号分隔" rows="2" :disabled="scanStore.scanning"></textarea>
        </div>
        <div class="excl-item">
          <label class="excl-label">排除文件名</label>
          <textarea v-model="scanStore.exclusions.names" class="excl-input" placeholder="如: thumbs.db, desktop.ini&#10;用逗号分隔" rows="2" :disabled="scanStore.scanning"></textarea>
        </div>
        <div class="excl-item">
          <label class="excl-label">排除后缀</label>
          <textarea v-model="scanStore.exclusions.exts" class="excl-input" placeholder="如: .tmp, .log, .bak&#10;用逗号分隔" rows="2" :disabled="scanStore.scanning"></textarea>
        </div>
      </div>
    </div>

    <!-- Start button -->
    <button class="btn-start" :disabled="scanStore.scanning" @click="startScan">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polygon points="5 3 19 12 5 21 5 3"/></svg>
      {{ scanStore.scanning ? '扫描中...' : '开始扫描' }}
    </button>

    <!-- Progress -->
    <div class="progress-card" v-if="scanStore.scanning || scanStore.progress > 0">
      <div class="progress-header">
        <span class="status-text">{{ scanStore.statusText }}</span>
        <span class="elapsed" v-if="scanStore.scanning">{{ formatElapsed(scanStore.elapsed) }}</span>
      </div>
      <div class="progress-bar">
        <div class="progress-fill" :style="{ width: scanStore.progress + '%' }"></div>
      </div>
      <div class="current-file" v-if="scanStore.currentFile && scanStore.scanning">
        <span class="file-label">正在扫描:</span>
        <span class="file-path">{{ scanStore.currentFile }}</span>
      </div>
    </div>

    <!-- Stats -->
    <div class="stats-grid">
      <div class="stat-card">
        <div class="stat-icon stat-icon-primary">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ fileStore.stats.total }}</div>
          <div class="stat-label">总文件数</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-icon stat-icon-danger">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="2" y="2" width="20" height="20" rx="2"/><path d="M12 8v8M8 12h8"/></svg>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ fileStore.stats.duplicates }}</div>
          <div class="stat-label">重复文件</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-icon stat-icon-info">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M12 2L2 7l10 5 10-5-10-5z"/><path d="M2 17l10 5 10-5"/><path d="M2 12l10 5 10-5"/></svg>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ fileStore.stats.multiversion }}</div>
          <div class="stat-label">多版本</div>
        </div>
      </div>
      <div class="stat-card">
        <div class="stat-icon stat-icon-warning">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><path d="M12 8v4M12 16h.01"/></svg>
        </div>
        <div class="stat-info">
          <div class="stat-value">{{ fileStore.stats.uncategorized }}</div>
          <div class="stat-label">待分类</div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.scan-view { display: flex; flex-direction: column; gap: 16px; }
.page-title { font-size: 20px; font-weight: 700; color: #1f2937; margin: 0; }

.scan-section {
  background: #fff; border-radius: 8px; padding: 16px 20px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}
.section-label { font-size: 13px; font-weight: 600; color: #374151; margin-bottom: 10px; }
.dir-list { display: flex; flex-direction: column; gap: 8px; }
.dir-row { display: flex; gap: 6px; align-items: center; }

.dir-input {
  flex: 1; padding: 8px 14px; border: 0.5px solid #d1d5db; border-radius: 6px;
  font-size: 13px; outline: none;
}
.dir-input:focus { border-color: #185FA5; box-shadow: 0 0 0 2px rgba(24,95,165,0.1); }

.btn-icon {
  width: 28px; height: 28px; display: flex; align-items: center; justify-content: center;
  border: none; border-radius: 4px; cursor: pointer; font-size: 16px; color: #9ca3af; background: transparent;
}
.btn-icon.remove:hover { background: #fee2e2; color: #A32D2D; }

.btn-add {
  padding: 6px 14px; border: 1px dashed #d1d5db; border-radius: 6px;
  background: transparent; font-size: 12px; color: #6b7280; cursor: pointer;
}
.btn-add:hover { border-color: #185FA5; color: #185FA5; background: #EFF6FF; }

.exclusion-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; }
.excl-item { display: flex; flex-direction: column; gap: 4px; }
.excl-label { font-size: 12px; color: #6b7280; font-weight: 500; }

.excl-input {
  padding: 6px 10px; border: 0.5px solid #d1d5db; border-radius: 6px;
  font-size: 12px; outline: none; resize: vertical; font-family: inherit;
}
.excl-input:focus { border-color: #185FA5; }

.btn-start {
  display: flex; align-items: center; justify-content: center; gap: 8px;
  padding: 10px 24px; background: #185FA5; color: #fff; border: none;
  border-radius: 8px; font-size: 14px; font-weight: 600; cursor: pointer;
  transition: background 0.15s;
}
.btn-start:hover { background: #144e8a; }
.btn-start:disabled { opacity: 0.6; cursor: not-allowed; }

.progress-card { background: #fff; border-radius: 8px; padding: 16px 20px; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
.progress-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
.status-text { font-size: 14px; font-weight: 600; color: #374151; }
.elapsed { font-size: 13px; color: #9ca3af; font-variant-numeric: tabular-nums; }
.progress-bar { height: 20px; background: #e5e7eb; border-radius: 10px; overflow: hidden; }
.progress-fill { height: 100%; background: #185FA5; border-radius: 10px; transition: width 0.3s; }
.current-file { display: flex; gap: 6px; margin-top: 8px; font-size: 12px; }
.file-label { color: #9ca3af; flex-shrink: 0; }
.file-path { color: #6b7280; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: monospace; }

.stats-grid { display: grid; grid-template-columns: repeat(4, 1fr); gap: 10px; }
.stat-card { background: #fff; border-radius: 8px; padding: 14px 16px; box-shadow: 0 1px 2px rgba(0,0,0,0.05); display: flex; align-items: center; gap: 12px; }
.stat-icon { width: 38px; height: 38px; border-radius: 8px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
.stat-icon-primary { background: #EFF6FF; color: #185FA5; }
.stat-icon-danger { background: #FEF2F2; color: #A32D2D; }
.stat-icon-info { background: #EFF6FF; color: #185FA5; }
.stat-icon-warning { background: #FFFBEB; color: #854F0B; }
.stat-info { display: flex; flex-direction: column; }
.stat-value { font-size: 22px; font-weight: 700; color: #1f2937; line-height: 1.2; }
.stat-label { font-size: 12px; color: #6b7280; margin-top: 2px; }
</style>
