<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NInput, NButton, useMessage } from 'naive-ui'
import axios from 'axios'

const message = useMessage()
const activeTab = ref<'func' | 'rules'>('func')

// ── AI Functional Categories ──────────────────────────────

interface FuncCategory {
  name: string
  keywords: string[]
}

const funcCategories = ref<FuncCategory[]>([])
const funcLoading = ref(false)
const funcSaving = ref(false)
const funcEditIdx = ref<number | null>(null)
const funcEditForm = ref<FuncCategory>({ name: '', keywords: [] })
const funcNewName = ref('')
const funcNewKeywords = ref('')

async function fetchFuncCategories() {
  funcLoading.value = true
  try {
    const resp = await axios.get('/api/func-categories')
    funcCategories.value = resp.data.data ?? []
  } catch {
    message.error('获取 AI 功能分类失败')
  } finally {
    funcLoading.value = false
  }
}

function startFuncEdit(idx: number) {
  const cat = funcCategories.value[idx]
  funcEditIdx.value = idx
  funcEditForm.value = { name: cat.name, keywords: [...cat.keywords] }
}

function cancelFuncEdit() {
  funcEditIdx.value = null
}

async function saveFuncEdit() {
  if (funcEditIdx.value === null) return
  funcCategories.value[funcEditIdx.value] = { ...funcEditForm.value }
  funcEditIdx.value = null
  await saveFuncCategories()
}

function removeFuncCategory(idx: number) {
  funcCategories.value.splice(idx, 1)
  saveFuncCategories()
}

async function addFuncCategory() {
  if (!funcNewName.value.trim()) {
    message.warning('请输入分类名称')
    return
  }
  funcCategories.value.push({
    name: funcNewName.value.trim(),
    keywords: funcNewKeywords.value.split(',').map(s => s.trim()).filter(Boolean),
  })
  funcNewName.value = ''
  funcNewKeywords.value = ''
  await saveFuncCategories()
}

async function saveFuncCategories() {
  funcSaving.value = true
  try {
    await axios.put('/api/func-categories', funcCategories.value)
    message.success('已保存')
  } catch {
    message.error('保存失败')
  } finally {
    funcSaving.value = false
  }
}

// ── File Type Rules ──────────────────────────────────────

interface CategoryRule {
  name: string
  target_path: string
  extensions: string[]
  name_keywords: string[]
}

const rules = ref<CategoryRule[]>([])
const rulesLoading = ref(false)
const rulesSaving = ref(false)
const rulesEditIdx = ref<number | null>(null)
const rulesEditForm = ref<CategoryRule>({ name: '', target_path: '', extensions: [], name_keywords: [] })
const rulesNewName = ref('')
const rulesNewTarget = ref('')
const rulesNewExts = ref('')
const rulesNewKw = ref('')

async function fetchRules() {
  rulesLoading.value = true
  try {
    const resp = await axios.get('/api/rules')
    rules.value = resp.data.data ?? []
  } catch {
    message.error('获取文件分类规则失败')
  } finally {
    rulesLoading.value = false
  }
}

function startRulesEdit(idx: number) {
  const r = rules.value[idx]
  rulesEditIdx.value = idx
  rulesEditForm.value = {
    name: r.name,
    target_path: r.target_path,
    extensions: [...r.extensions],
    name_keywords: [...r.name_keywords],
  }
}

function cancelRulesEdit() {
  rulesEditIdx.value = null
}

async function saveRulesEdit() {
  if (rulesEditIdx.value === null) return
  rules.value[rulesEditIdx.value] = { ...rulesEditForm.value }
  rulesEditIdx.value = null
  await saveRules()
}

function removeRule(idx: number) {
  rules.value.splice(idx, 1)
  saveRules()
}

async function addRule() {
  if (!rulesNewName.value.trim()) {
    message.warning('请输入分类名称')
    return
  }
  rules.value.push({
    name: rulesNewName.value.trim(),
    target_path: rulesNewTarget.value.trim(),
    extensions: rulesNewExts.value.split(',').map(s => s.trim().toLowerCase()).filter(Boolean),
    name_keywords: rulesNewKw.value.split(',').map(s => s.trim()).filter(Boolean),
  })
  rulesNewName.value = ''
  rulesNewTarget.value = ''
  rulesNewExts.value = ''
  rulesNewKw.value = ''
  await saveRules()
}

async function saveRules() {
  rulesSaving.value = true
  try {
    await axios.put('/api/rules', rules.value)
    message.success('已保存')
  } catch {
    message.error('保存失败')
  } finally {
    rulesSaving.value = false
  }
}

// ── Init ──────────────────────────────────────────────────

onMounted(() => {
  fetchFuncCategories()
  fetchRules()
})
</script>

<template>
  <div class="categories-view">
    <h2 class="page-title">分类管理</h2>

    <!-- Tab switcher -->
    <div class="tab-bar">
      <button class="tab-btn" :class="{ active: activeTab === 'func' }" @click="activeTab = 'func'">AI 功能分类</button>
      <button class="tab-btn" :class="{ active: activeTab === 'rules' }" @click="activeTab = 'rules'">文件类型分类</button>
    </div>

    <!-- ═══ Tab 1: AI Functional Categories ═══ -->
    <template v-if="activeTab === 'func'">
      <div class="section-card">
        <h3 class="card-title">AI 功能分类</h3>
        <p class="card-desc">定义 AI 丰富时可选择的功能分类，所有 AI 生成的分类将限定在此范围内，无法匹配则归入 "others"。</p>

        <!-- Add new -->
        <div class="add-row">
          <n-input v-model:value="funcNewName" placeholder="分类名称 (如: 开发工具)" style="width:180px" size="small" />
          <n-input v-model:value="funcNewKeywords" placeholder="关键词 (逗号分隔)" style="width:300px" size="small" />
          <n-button type="primary" size="small" @click="addFuncCategory" :loading="funcSaving">添加</n-button>
        </div>

        <table class="cat-table" v-if="!funcLoading">
          <thead>
            <tr>
              <th>分类名称</th>
              <th>关键词</th>
              <th style="width:120px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(cat, idx) in funcCategories" :key="cat.name + idx">
              <template v-if="funcEditIdx === idx">
                <td><n-input v-model:value="funcEditForm.name" size="small" style="width:160px" /></td>
                <td><n-input :value="funcEditForm.keywords.join(', ')" @input="(v: string) => funcEditForm.keywords = v.split(',').map(s => s.trim()).filter(Boolean)" size="small" placeholder="关键词逗号分隔" /></td>
                <td>
                  <div class="action-row">
                    <button class="btn-sm success" @click="saveFuncEdit">保存</button>
                    <button class="btn-sm" @click="cancelFuncEdit">取消</button>
                  </div>
                </td>
              </template>
              <template v-else>
                <td><span class="cat-name">{{ cat.name }}</span></td>
                <td>
                  <span v-for="kw in cat.keywords" :key="kw" class="kw-pill">{{ kw }}</span>
                  <span v-if="!cat.keywords || cat.keywords.length === 0" class="empty-hint">—</span>
                </td>
                <td>
                  <div class="action-row">
                    <button class="btn-sm primary" @click="startFuncEdit(idx)">编辑</button>
                    <button class="btn-sm danger" @click="removeFuncCategory(idx)">删除</button>
                  </div>
                </td>
              </template>
            </tr>
            <tr v-if="funcCategories.length === 0">
              <td colspan="3" class="empty-cell">暂无 AI 功能分类</td>
            </tr>
          </tbody>
        </table>
        <div v-else class="loading-cell">加载中...</div>
      </div>
    </template>

    <!-- ═══ Tab 2: File Type Rules ═══ -->
    <template v-if="activeTab === 'rules'">
      <div class="section-card">
        <h3 class="card-title">文件类型分类</h3>
        <p class="card-desc">定义文件扫描时的自动分类规则，基于扩展名和文件名关键词匹配。</p>

        <!-- Add new -->
        <div class="add-row">
          <n-input v-model:value="rulesNewName" placeholder="分类名称" style="width:120px" size="small" />
          <n-input v-model:value="rulesNewTarget" placeholder="目标文件夹" style="width:120px" size="small" />
          <n-input v-model:value="rulesNewExts" placeholder="扩展名 (.exe,.msi)" style="width:180px" size="small" />
          <n-input v-model:value="rulesNewKw" placeholder="关键词 (可选)" style="width:160px" size="small" />
          <n-button type="primary" size="small" @click="addRule" :loading="rulesSaving">添加</n-button>
        </div>

        <table class="cat-table" v-if="!rulesLoading">
          <thead>
            <tr>
              <th>分类名称</th>
              <th>目标路径</th>
              <th>扩展名</th>
              <th>关键词</th>
              <th style="width:120px">操作</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(rule, idx) in rules" :key="rule.name + idx">
              <template v-if="rulesEditIdx === idx">
                <td><n-input v-model:value="rulesEditForm.name" size="small" style="width:100px" /></td>
                <td><n-input v-model:value="rulesEditForm.target_path" size="small" style="width:100px" /></td>
                <td><n-input :value="rulesEditForm.extensions.join(', ')" @input="(v: string) => rulesEditForm.extensions = v.split(',').map(s => s.trim()).filter(Boolean)" size="small" /></td>
                <td><n-input :value="rulesEditForm.name_keywords.join(', ')" @input="(v: string) => rulesEditForm.name_keywords = v.split(',').map(s => s.trim()).filter(Boolean)" size="small" /></td>
                <td>
                  <div class="action-row">
                    <button class="btn-sm success" @click="saveRulesEdit">保存</button>
                    <button class="btn-sm" @click="cancelRulesEdit">取消</button>
                  </div>
                </td>
              </template>
              <template v-else>
                <td><span class="cat-name">{{ rule.name }}</span></td>
                <td><span class="target-tag">{{ rule.target_path || '—' }}</span></td>
                <td>
                  <span v-for="ext in rule.extensions" :key="ext" class="ext-pill">{{ ext }}</span>
                  <span v-if="!rule.extensions || rule.extensions.length === 0" class="empty-hint">—</span>
                </td>
                <td>
                  <span v-for="kw in rule.name_keywords" :key="kw" class="kw-pill">{{ kw }}</span>
                  <span v-if="!rule.name_keywords || rule.name_keywords.length === 0" class="empty-hint">—</span>
                </td>
                <td>
                  <div class="action-row">
                    <button class="btn-sm primary" @click="startRulesEdit(idx)">编辑</button>
                    <button class="btn-sm danger" @click="removeRule(idx)">删除</button>
                  </div>
                </td>
              </template>
            </tr>
            <tr v-if="rules.length === 0">
              <td colspan="5" class="empty-cell">暂无文件分类规则</td>
            </tr>
          </tbody>
        </table>
        <div v-else class="loading-cell">加载中...</div>
      </div>
    </template>
  </div>
</template>

<style scoped>
.categories-view { display: flex; flex-direction: column; gap: 16px; }
.page-title { font-size: 20px; font-weight: 700; color: #1f2937; margin: 0; }

.tab-bar { display: flex; gap: 0; background: #fff; border-radius: 8px; overflow: hidden; box-shadow: 0 1px 2px rgba(0,0,0,0.05); }
.tab-btn {
  padding: 10px 20px; border: none; background: none; font-size: 13px; font-weight: 600;
  color: #6b7280; cursor: pointer; border-bottom: 2px solid transparent; transition: all 0.15s;
}
.tab-btn:hover { color: #374151; background: #f9fafb; }
.tab-btn.active { color: #185FA5; border-bottom-color: #185FA5; background: #E6F1FB; }

.section-card {
  background: #fff; border-radius: 8px; padding: 20px 24px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.card-title { font-size: 15px; font-weight: 700; color: #1f2937; margin: 0 0 6px; }
.card-desc { font-size: 12px; color: #9ca3af; margin: 0 0 16px; line-height: 1.5; }

.add-row { display: flex; gap: 8px; align-items: center; margin-bottom: 16px; flex-wrap: wrap; }

.cat-table { width: 100%; border-collapse: collapse; font-size: 13px; }
.cat-table th {
  text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600;
  color: #9ca3af; text-transform: uppercase; background: #fafafa;
  border-bottom: 0.5px solid #e5e7eb;
}
.cat-table td { padding: 8px 12px; border-bottom: 0.5px solid #f3f4f6; vertical-align: middle; }
.cat-table tr:hover { background: #f9fafb; }

.cat-name { font-weight: 600; color: #1f2937; }
.target-tag { color: #6b7280; font-size: 12px; font-family: monospace; }

.kw-pill {
  display: inline-block; padding: 1px 7px; border-radius: 10px; font-size: 11px;
  background: #E6F1FB; color: #185FA5; margin-right: 3px; margin-bottom: 2px;
}
.ext-pill {
  display: inline-block; padding: 1px 6px; border-radius: 4px; font-size: 11px;
  background: #F3F4F6; color: #374151; font-family: monospace; margin-right: 3px; margin-bottom: 2px;
  border: 0.5px solid #E5E7EB;
}

.empty-hint { color: #D1D5DB; font-size: 12px; }
.empty-cell { text-align: center; padding: 32px 20px !important; color: #9ca3af; font-size: 13px; }
.loading-cell { text-align: center; padding: 32px; color: #9ca3af; font-size: 13px; }

.action-row { display: flex; gap: 6px; }
.btn-sm {
  padding: 3px 10px; border-radius: 4px; font-size: 11px; cursor: pointer;
  border: 1px solid #d1d5db; background: #fff; color: #374151;
}
.btn-sm:hover { background: #f3f4f6; }
.btn-sm.primary { color: #185FA5; border-color: #93c5fd; }
.btn-sm.primary:hover { background: #eff6ff; }
.btn-sm.success { color: #3B6D11; border-color: #86efac; }
.btn-sm.success:hover { background: #f0fdf4; }
.btn-sm.danger { color: #A32D2D; border-color: #fca5a5; }
.btn-sm.danger:hover { background: #fef2f2; }
</style>
