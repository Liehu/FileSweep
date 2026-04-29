<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { NInput, NButton, useMessage } from 'naive-ui'
import axios from 'axios'

const message = useMessage()

interface TagDef {
  id: string
  name: string
  color: string
  description: string
  count: number
}

const tags = ref<TagDef[]>([])
const loading = ref(false)
const saving = ref(false)
const newTagName = ref('')
const newTagColor = ref('#185FA5')
const newTagDesc = ref('')
const editingId = ref<string | null>(null)

const PRESET_COLORS = [
  '#185FA5', '#3B6D11', '#A32D2D', '#854F0B',
  '#7C3AED', '#0891B2', '#D97706', '#059669',
  '#DC2626', '#7C3AED', '#2563EB', '#EA580C',
]

onMounted(() => { fetchTags() })

async function fetchTags() {
  loading.value = true
  try {
    const resp = await axios.get('/api/tags')
    const body = resp.data
    tags.value = body.data ?? body.items ?? (Array.isArray(body) ? body : [])
  } catch {
    message.error('获取标签失败')
  } finally {
    loading.value = false
  }
}

function startEdit(tag: TagDef) {
  editingId.value = tag.id
}

function cancelEdit() {
  editingId.value = null
}

async function saveTag(tag: TagDef) {
  try {
    await axios.put(`/api/tags/${tag.id}`, {
      name: tag.name,
      color: tag.color,
      description: tag.description,
    })
    message.success('标签已保存')
    editingId.value = null
  } catch {
    message.error('保存失败')
  }
}

async function deleteTag(id: string) {
  try {
    await axios.delete(`/api/tags/${id}`)
    tags.value = tags.value.filter(t => t.id !== id)
    message.success('标签已删除')
  } catch {
    message.error('删除失败')
  }
}

async function createTag() {
  if (!newTagName.value.trim()) {
    message.warning('请输入标签名称')
    return
  }
  saving.value = true
  try {
    const resp = await axios.post('/api/tags', {
      name: newTagName.value.trim(),
      color: newTagColor.value,
      description: newTagDesc.value,
    })
    const created: TagDef = {
      id: resp.data.id,
      name: newTagName.value.trim(),
      color: newTagColor.value,
      description: newTagDesc.value,
      count: 0,
    }
    tags.value.unshift(created)
    newTagName.value = ''
    newTagColor.value = '#185FA5'
    newTagDesc.value = ''
    message.success('标签已创建')
  } catch {
    message.error('创建失败')
  } finally {
    saving.value = false
  }
}

const tagCount = computed(() => tags.value.length)
</script>

<template>
  <div class="tags-view">
    <h2 class="page-title">标签管理</h2>

    <!-- Create tag -->
    <div class="create-card">
      <h3 class="card-title">新建标签</h3>
      <div class="create-form">
        <div class="form-field">
          <label class="field-label">标签名称 <span class="required">*</span></label>
          <n-input v-model:value="newTagName" placeholder="如：安全工具、开发框架" style="max-width:220px" />
        </div>
        <div class="form-field">
          <label class="field-label">标签颜色</label>
          <div class="color-picker">
            <input type="color" v-model="newTagColor" class="color-input" />
            <div class="preset-colors">
              <button
                v-for="c in PRESET_COLORS"
                :key="c"
                class="preset-dot"
                :style="{ background: c, outline: newTagColor === c ? '2px solid #1f2937' : 'none' }"
                @click="newTagColor = c"
              />
            </div>
          </div>
        </div>
        <div class="form-field">
          <label class="field-label">描述（可选）</label>
          <n-input v-model:value="newTagDesc" placeholder="标签用途说明" style="max-width:300px" />
        </div>
        <div class="form-field form-field-action">
          <label class="field-label">&nbsp;</label>
          <n-button type="primary" :loading="saving" @click="createTag">
            + 创建标签
          </n-button>
        </div>
      </div>
    </div>

    <!-- Tag list -->
    <div class="table-card">
      <div class="table-header">
        <h3 class="card-title">已有标签 ({{ tagCount }})</h3>
        <span class="hint-text">标签用于 AI 丰富时的分类约束，所有 AI 生成的标签将限定在此范围内</span>
      </div>

      <div v-if="loading" class="loading-cell">加载中...</div>

      <table class="tags-table" v-else>
        <thead>
          <tr>
            <th style="width:40px">颜色</th>
            <th>标签名称</th>
            <th>描述</th>
            <th style="width:80px">引用数</th>
            <th style="width:140px">操作</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="tag in tags" :key="tag.id">
            <td>
              <div class="tag-color-dot" :style="{ background: tag.color }"></div>
            </td>
            <td>
              <template v-if="editingId === tag.id">
                <div class="inline-edit">
                  <n-input v-model:value="tag.name" size="small" style="width:140px" />
                  <input type="color" v-model="tag.color" class="color-input-sm" />
                </div>
              </template>
              <template v-else>
                <span class="tag-pill" :style="{ background: tag.color + '22', color: tag.color, border: `1px solid ${tag.color}55` }">
                  {{ tag.name }}
                </span>
              </template>
            </td>
            <td>
              <template v-if="editingId === tag.id">
                <n-input v-model:value="tag.description" size="small" placeholder="描述" />
              </template>
              <template v-else>
                <span class="desc-text">{{ tag.description || '—' }}</span>
              </template>
            </td>
            <td class="count-cell">{{ tag.count }}</td>
            <td>
              <template v-if="editingId === tag.id">
                <div class="action-row">
                  <button class="btn-sm success" @click="saveTag(tag)">保存</button>
                  <button class="btn-sm" @click="cancelEdit">取消</button>
                </div>
              </template>
              <template v-else>
                <div class="action-row">
                  <button class="btn-sm primary" @click="startEdit(tag)">编辑</button>
                  <button class="btn-sm danger" @click="deleteTag(tag.id)">删除</button>
                </div>
              </template>
            </td>
          </tr>
          <tr v-if="tags.length === 0 && !loading">
            <td colspan="5" class="empty-cell">暂无标签，请先创建标签以约束 AI 丰富的分类范围</td>
          </tr>
        </tbody>
      </table>
    </div>

    <!-- Info box -->
    <div class="info-box">
      <div class="info-icon">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <circle cx="12" cy="12" r="10"/><path d="M12 8v4m0 4h.01"/>
        </svg>
      </div>
      <div class="info-content">
        <strong>关于标签约束</strong><br>
        在 AI 丰富页面执行丰富任务时，AI 生成的标签将严格限定在此处定义的标签范围内。
        若软件无法归入任何已有标签，将自动标记为 <code>others</code>。
        分类（功能分类）同理，仅从 <code>config/categories.yaml</code> 中定义的类别中选取。
      </div>
    </div>
  </div>
</template>

<style scoped>
.tags-view { display: flex; flex-direction: column; gap: 16px; }
.page-title { font-size: 20px; font-weight: 700; color: #1f2937; margin: 0; }

.create-card, .table-card {
  background: #fff; border-radius: 8px; padding: 20px 24px;
  box-shadow: 0 1px 2px rgba(0,0,0,0.05);
}

.card-title { font-size: 15px; font-weight: 700; color: #1f2937; margin: 0 0 14px; }

.create-form {
  display: flex; flex-wrap: wrap; gap: 16px; align-items: flex-end;
}

.form-field { display: flex; flex-direction: column; gap: 6px; }
.form-field-action { justify-content: flex-end; }

.field-label { font-size: 12px; font-weight: 600; color: #6b7280; }
.required { color: #dc2626; }

.color-picker { display: flex; align-items: center; gap: 8px; }
.color-input { width: 36px; height: 28px; border: 1px solid #d1d5db; border-radius: 4px; cursor: pointer; padding: 2px; background: none; }
.color-input-sm { width: 28px; height: 24px; border: 1px solid #d1d5db; border-radius: 4px; cursor: pointer; padding: 1px; }

.preset-colors { display: flex; gap: 6px; flex-wrap: wrap; max-width: 200px; }
.preset-dot {
  width: 20px; height: 20px; border-radius: 50%; border: none; cursor: pointer;
  transition: transform 0.1s;
}
.preset-dot:hover { transform: scale(1.2); }

.table-header {
  display: flex; align-items: center; justify-content: space-between; margin-bottom: 14px;
}
.hint-text { font-size: 12px; color: #9ca3af; }

.tags-table { width: 100%; border-collapse: collapse; font-size: 13px; }
.tags-table th {
  text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600;
  color: #9ca3af; text-transform: uppercase; background: #fafafa;
  border-bottom: 0.5px solid #e5e7eb;
}
.tags-table td { padding: 10px 12px; border-bottom: 0.5px solid #f3f4f6; }
.tags-table tr:hover { background: #f9fafb; }

.tag-color-dot { width: 16px; height: 16px; border-radius: 50%; }

.tag-pill {
  display: inline-block; padding: 2px 10px; border-radius: 12px;
  font-size: 12px; font-weight: 600;
}

.desc-text { color: #6b7280; font-size: 12px; }
.count-cell { color: #9ca3af; text-align: center; }

.inline-edit { display: flex; align-items: center; gap: 6px; }

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

.loading-cell, .empty-cell {
  text-align: center; padding: 32px; color: #9ca3af; font-size: 13px;
}

.info-box {
  display: flex; gap: 12px; padding: 14px 16px;
  background: #E6F1FB; border-radius: 8px; border-left: 3px solid #185FA5;
}
.info-icon { color: #185FA5; flex-shrink: 0; margin-top: 2px; }
.info-content { font-size: 13px; color: #374151; line-height: 1.6; }
.info-content code {
  background: #dbeafe; padding: 1px 6px; border-radius: 4px;
  font-family: monospace; font-size: 12px; color: #1e40af;
}
</style>
