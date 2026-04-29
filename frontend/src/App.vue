<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useRouter, useRoute } from 'vue-router'
import { NConfigProvider, NMessageProvider } from 'naive-ui'
import axios from 'axios'

const router = useRouter()
const route = useRoute()

interface NavItem {
  label: string
  icon: string
  route: string
}

const mainNavItems: NavItem[] = [
  { label: '全部文件', icon: 'folder', route: '/files' },
  { label: '重复文件', icon: 'copy', route: '/files?dup=1' },
  { label: '多版本', icon: 'layers', route: '/files?mv=1' },
]

interface RuleCategory {
  name: string
  target_path: string
  extensions: string[]
  name_keywords: string[]
}
const ruleCategories = ref<RuleCategory[]>([])

interface CatNode {
  label: string
  route: string
  children: CatNode[]
}

const categoryTree = computed<CatNode[]>(() => {
  const roots: CatNode[] = []
  for (const cat of ruleCategories.value) {
    const parts = cat.name.split('\\')
    let list = roots
    let fullPath = ''
    for (let i = 0; i < parts.length; i++) {
      const part = parts[i]
      fullPath += (i > 0 ? '\\' : '') + part
      let node = list.find(n => n.label === part)
      if (!node) {
        node = { label: part, route: '/files?cat=' + encodeURIComponent(fullPath), children: [] }
        list.push(node)
      }
      list = node.children
    }
  }
  return roots
})

const bottomNavItems: NavItem[] = [
  { label: '扫描', icon: 'search', route: '/scan' },
  { label: '软件目录', icon: 'book-open', route: '/catalog' },
  { label: 'AI 丰富', icon: 'sparkles', route: '/enrich' },
  { label: '分类管理', icon: 'folder', route: '/categories' },
  { label: '标签管理', icon: 'tag', route: '/tags' },
  { label: '操作日志', icon: 'list', route: '/logs' },
  { label: '设置', icon: 'settings', route: '/settings' },
]

const rightPanelOpen = ref(true)

const currentRoute = computed(() => route.path)
const currentQuery = computed(() => route.fullPath)

function isActive(navRoute: string): boolean {
  if (navRoute.includes('?')) {
    return currentQuery.value === navRoute
  }
  return currentRoute.value === navRoute
}

function navigate(navRoute: string) {
  router.push(navRoute)
}

function toggleRightPanel() {
  rightPanelOpen.value = !rightPanelOpen.value
}

interface RuleItem {
  key: string
  label: string
  enabled: boolean
}

const rules = ref<RuleItem[]>([
  { key: 'autoCategorize', label: '安装包归类', enabled: true },
  { key: 'autoDuplicate', label: '自动去重', enabled: true },
  { key: 'keepNewestVersion', label: '版本保留最新', enabled: true },
  { key: 'moveToRecycleBin', label: '移至回收站', enabled: true },
  { key: 'deleteEmptyDirs', label: '删除空目录', enabled: false },
])

async function saveRules() {
  const rulesMap: Record<string, unknown> = {}
  for (const r of rules.value) {
    rulesMap[r.key] = r.enabled
  }
  try {
    const resp = await axios.get('/api/settings')
    const merged = { ...(resp.data?.rules || {}), ...rulesMap }
    await axios.put('/api/settings', { rules: merged })
  } catch { /* ignore */ }
}

function toggleRule(rule: RuleItem) {
  rule.enabled = !rule.enabled
  saveRules()
}

onMounted(async () => {
  try {
    const [rulesResp, settingsResp] = await Promise.all([
      axios.get('/api/rules'),
      axios.get('/api/settings'),
    ])
    ruleCategories.value = rulesResp.data.data ?? []
    const backendRules = settingsResp.data?.rules
    if (backendRules) {
      for (const r of rules.value) {
        if (typeof backendRules[r.key] === 'boolean') {
          r.enabled = backendRules[r.key]
        }
      }
    }
  } catch { /* ignore */ }
})
</script>

<template>
  <n-config-provider>
    <n-message-provider>
  <div class="app-layout">
    <!-- Left Sidebar -->
    <aside class="sidebar">
      <div class="sidebar-inner">
        <!-- Logo -->
        <div class="sidebar-logo" @click="navigate('/files')">
          <svg class="logo-icon" viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2">
            <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
          </svg>
          <span class="logo-text">FileSweep</span>
        </div>

        <!-- Main Navigation -->
        <nav class="nav-section">
          <div class="nav-section-title">文件</div>
          <a
            v-for="item in mainNavItems"
            :key="item.route"
            class="nav-item"
            :class="{ active: isActive(item.route) }"
            @click.prevent="navigate(item.route)"
          >
            <span class="nav-label">{{ item.label }}</span>
          </a>
        </nav>

        <!-- Category Navigation -->
        <nav class="nav-section">
          <div class="nav-section-title">分类</div>
          <template v-for="node in categoryTree" :key="node.label">
            <a
              class="nav-item"
              :class="{ active: node.route && isActive(node.route) }"
              @click.prevent="node.route && navigate(node.route)"
            >
              <span class="nav-label">{{ node.label }}</span>
            </a>
            <a
              v-for="child in node.children"
              :key="child.label"
              class="nav-item nav-sub"
              :class="{ active: child.route && isActive(child.route) }"
              @click.prevent="child.route && navigate(child.route)"
            >
              <span class="nav-label">{{ child.label }}</span>
            </a>
          </template>
        </nav>

        <!-- Bottom Navigation -->
        <nav class="nav-section nav-bottom">
          <a
            v-for="item in bottomNavItems"
            :key="item.route"
            class="nav-item"
            :class="{ active: isActive(item.route) }"
            @click.prevent="navigate(item.route)"
          >
            <span class="nav-label">{{ item.label }}</span>
          </a>
        </nav>
      </div>
    </aside>

    <!-- Main Content -->
    <main class="main-area">
      <div class="main-scroll">
        <router-view />
      </div>
    </main>

    <!-- Right Panel -->
    <aside v-if="rightPanelOpen" class="right-panel">
      <div class="right-panel-inner">
        <div class="right-panel-header">
          <h3 class="right-panel-title">整理规则</h3>
          <button class="btn-icon" @click="toggleRightPanel" title="关闭面板">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>
        <div class="rules-list">
          <div v-for="rule in rules" :key="rule.key" class="rule-item">
            <label class="rule-toggle" @click.prevent="toggleRule(rule)">
              <input type="checkbox" :checked="rule.enabled" class="toggle-input" />
              <span class="toggle-slider" :class="{ on: rule.enabled }"></span>
            </label>
            <span class="rule-label" :class="{ dimmed: !rule.enabled }">{{ rule.label }}</span>
          </div>
        </div>
      </div>
    </aside>

    <!-- Toggle button when panel is closed -->
    <button v-if="!rightPanelOpen" class="panel-toggle-btn" @click="toggleRightPanel" title="打开整理规则">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M12 3h7a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-7m0-18H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h7m0-18v18" />
      </svg>
    </button>
  </div>
    </n-message-provider>
  </n-config-provider>
</template>

<style scoped>
.app-layout {
  display: flex;
  height: 100vh;
  overflow: hidden;
}

/* Sidebar */
.sidebar {
  width: 200px;
  min-width: 200px;
  background: #fff;
  border-right: 1px solid #e5e5e5;
  display: flex;
  flex-direction: column;
}

.sidebar-inner {
  display: flex;
  flex-direction: column;
  height: 100%;
  padding: 0;
}

.sidebar-logo {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 16px 16px 12px;
  cursor: pointer;
  user-select: none;
}

.logo-icon {
  color: var(--color-primary);
}

.logo-text {
  font-size: 18px;
  font-weight: 700;
  color: var(--color-primary);
  letter-spacing: -0.5px;
}

.nav-section {
  padding: 4px 8px;
}

.nav-section-title {
  font-size: 11px;
  font-weight: 600;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.5px;
  padding: 12px 8px 4px;
}

.nav-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  border-radius: 6px;
  cursor: pointer;
  font-size: 13px;
  color: #4b5563;
  transition: all 0.15s;
  user-select: none;
  text-decoration: none;
}

.nav-item:hover {
  background: #f3f4f6;
  color: #1f2937;
}

.nav-item.active {
  background: var(--color-info-bg);
  color: var(--color-primary);
  font-weight: 600;
}

.nav-sub {
  padding-left: 28px;
  font-size: 12px;
}

.nav-bottom {
  margin-top: auto;
  border-top: 1px solid #e5e5e5;
  padding-top: 8px;
}

/* Main Area */
.main-area {
  flex: 1;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  background: #f5f5f5;
}

.main-scroll {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
}

/* Right Panel */
.right-panel {
  width: 210px;
  min-width: 210px;
  background: #fff;
  border-left: 1px solid #e5e5e5;
  display: flex;
  flex-direction: column;
}

.right-panel-inner {
  padding: 16px 12px;
  display: flex;
  flex-direction: column;
  height: 100%;
}

.right-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 16px;
}

.right-panel-title {
  font-size: 14px;
  font-weight: 700;
  color: #1f2937;
  margin: 0;
}

.btn-icon {
  background: none;
  border: none;
  cursor: pointer;
  color: #9ca3af;
  padding: 2px;
  border-radius: 4px;
  display: flex;
  align-items: center;
}

.btn-icon:hover {
  background: #f3f4f6;
  color: #4b5563;
}

.rules-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.rule-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 6px 4px;
}

.rule-toggle {
  position: relative;
  display: inline-block;
  width: 36px;
  height: 20px;
  flex-shrink: 0;
}

.toggle-input {
  opacity: 0;
  width: 0;
  height: 0;
}

.toggle-slider {
  position: absolute;
  cursor: pointer;
  top: 0;
  left: 0;
  right: 0;
  bottom: 0;
  background: #d1d5db;
  border-radius: 20px;
  transition: 0.2s;
}

.toggle-slider::before {
  content: '';
  position: absolute;
  height: 16px;
  width: 16px;
  left: 2px;
  bottom: 2px;
  background: #fff;
  border-radius: 50%;
  transition: 0.2s;
}

.toggle-slider.on {
  background: var(--color-primary);
}

.toggle-slider.on::before {
  transform: translateX(16px);
}

.rule-label {
  font-size: 13px;
  color: #374151;
}

.rule-label.dimmed {
  color: #9ca3af;
}

.panel-toggle-btn {
  position: fixed;
  right: 12px;
  top: 12px;
  background: #fff;
  border: 1px solid #e5e5e5;
  border-radius: 6px;
  padding: 6px 8px;
  cursor: pointer;
  color: #6b7280;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
  z-index: 10;
}

.panel-toggle-btn:hover {
  background: #f9fafb;
  color: #374151;
}
</style>
