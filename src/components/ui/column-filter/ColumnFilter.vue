<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, watch } from "vue";
import { Checkbox } from "@/components/ui/checkbox";
import { Filter } from "lucide-vue-next";

interface Props {
  /** 该字段所有可选值列表 */
  options: string[];
  /** 当前已选中的值（空数组 = 无筛选） */
  modelValue: string[];
  /** 是否单选模式（选中一个值即筛选；再点同一个取消） */
  single?: boolean;
}

const props = withDefaults(defineProps<Props>(), {
  single: false,
});

const emit = defineEmits<{
  "update:modelValue": [value: string[]];
}>();

const open = ref(false);
const triggerRef = ref<HTMLElement | null>(null);
const panelRef = ref<HTMLElement | null>(null);
const search = ref("");

const filteredOptions = computed(() => {
  const q = search.value.trim().toLowerCase();
  if (!q) return props.options;
  return props.options.filter((o) => o.toLowerCase().includes(q));
});

const active = computed(() => props.modelValue.length > 0);

function toggleValue(val: string) {
  if (props.single) {
    // 单选：选中同一个则取消
    const next = props.modelValue[0] === val ? [] : [val];
    emit("update:modelValue", next);
  } else {
    const set = new Set(props.modelValue);
    set.has(val) ? set.delete(val) : set.add(val);
    emit("update:modelValue", Array.from(set));
  }
}

function selectAll() {
  emit("update:modelValue", [...props.options]);
}

function clearAll() {
  emit("update:modelValue", []);
}

function onClickOutside(e: MouseEvent) {
  if (!open.value) return;
  const target = e.target as Node;
  if (
    triggerRef.value?.contains(target) ||
    panelRef.value?.contains(target)
  ) {
    return;
  }
  open.value = false;
}

onMounted(() => {
  document.addEventListener("click", onClickOutside);
});
onUnmounted(() => {
  document.removeEventListener("click", onClickOutside);
});

watch(open, (v) => {
  if (!v) search.value = "";
});
</script>

<template>
  <div class="relative inline-flex">
    <button
      ref="triggerRef"
      :class="[
        'inline-flex items-center gap-0.5 rounded transition-colors',
        active ? 'text-primary' : 'text-muted-foreground hover:text-foreground',
      ]"
      @click="open = !open"
    >
      <Filter class="h-3 w-3" />
    </button>

    <div
      v-if="open"
      ref="panelRef"
      class="absolute z-50 top-full left-0 mt-1 w-[200px] bg-popover text-popover-foreground border rounded-md shadow-lg"
    >
      <!-- 搜索框 -->
      <div class="p-2 border-b">
        <input
          v-model="search"
          placeholder="搜索..."
          class="w-full h-7 px-2 text-xs rounded border bg-transparent focus:outline-none focus:ring-1 focus:ring-ring"
        />
      </div>

      <!-- 值列表 -->
      <div class="max-h-[240px] overflow-y-auto py-1">
        <div
          v-if="filteredOptions.length === 0"
          class="px-3 py-2 text-xs text-muted-foreground"
        >
          无可选项
        </div>
        <label
          v-for="opt in filteredOptions"
          :key="opt"
          class="flex items-center gap-2 px-3 py-1.5 hover:bg-accent cursor-pointer text-xs"
        >
          <Checkbox
            :model-value="modelValue.includes(opt)"
            @update:model-value="() => toggleValue(opt)"
          />
          <span class="truncate flex-1" :title="opt">{{ opt || "(空)" }}</span>
        </label>
      </div>

      <!-- 底部操作 -->
      <div class="flex items-center justify-between border-t px-2 py-1.5" v-if="!single">
        <button class="text-[10px] text-muted-foreground hover:text-foreground" @click="selectAll">
          全选
        </button>
        <button class="text-[10px] text-muted-foreground hover:text-foreground" @click="clearAll">
          清除
        </button>
      </div>
      <div class="flex items-center justify-end border-t px-2 py-1.5" v-else>
        <button class="text-[10px] text-muted-foreground hover:text-foreground" @click="clearAll">
          清除
        </button>
      </div>
    </div>
  </div>
</template>
