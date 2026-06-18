<script setup lang="ts">
import { ref, computed, watch, nextTick } from "vue";
import { useRouter } from "vue-router";
import { getAllFeatures, type SearchableFeature } from "@/lib/plugin";
import { getIcon } from "./iconMap";

const props = defineProps<{ open: boolean }>();
const emit = defineEmits<{ "update:open": [boolean] }>();

const query = ref("");
const selectedIndex = ref(0);
const inputEl = ref<HTMLInputElement | null>(null);
const router = useRouter();

const results = computed<SearchableFeature[]>(() => {
  const q = query.value.trim().toLowerCase();
  const all = getAllFeatures();
  if (!q) return all;
  return all.filter(
    (f) =>
      f.cmds.some((c) => c.toLowerCase().includes(q)) ||
      f.explain.toLowerCase().includes(q) ||
      f.pluginName.toLowerCase().includes(q),
  );
});

watch(
  () => props.open,
  async (open) => {
    if (open) {
      query.value = "";
      selectedIndex.value = 0;
      await nextTick();
      inputEl.value?.focus();
    }
  },
);

watch(results, () => {
  selectedIndex.value = 0;
});

function activate(feature: SearchableFeature) {
  // route 类型：跳转路由；action 类型未来：直接执行命令（P1 全是 route）
  if (feature.route) {
    router.push(feature.route);
  }
  emit("update:open", false);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex.value = (selectedIndex.value + 1) % results.value.length;
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex.value =
      (selectedIndex.value - 1 + results.value.length) % results.value.length;
  } else if (e.key === "Enter") {
    e.preventDefault();
    const target = results.value[selectedIndex.value];
    if (target) activate(target);
  } else if (e.key === "Escape") {
    emit("update:open", false);
  }
}
</script>

<template>
  <div
    v-if="open"
    class="fixed inset-0 z-50 flex items-start justify-center pt-[15vh] bg-black/40"
    @click.self="emit('update:open', false)"
  >
    <div class="w-[480px] max-w-[90vw] bg-card rounded-lg shadow-xl border overflow-hidden">
      <input
        ref="inputEl"
        v-model="query"
        @keydown="onKeydown"
        placeholder="输入关键词搜索功能…"
        class="w-full px-4 py-3 bg-transparent border-b outline-none text-sm"
      />
      <div class="max-h-[320px] overflow-auto">
        <div
          v-for="(f, i) in results"
          :key="f.pluginId + f.code"
          :class="[
            'flex items-center gap-3 px-4 py-2.5 cursor-pointer text-sm',
            i === selectedIndex ? 'bg-accent' : 'hover:bg-accent/50',
          ]"
          @click="activate(f)"
          @mouseenter="selectedIndex = i"
        >
          <component :is="getIcon(f.pluginIcon)" class="h-4 w-4 text-primary shrink-0" />
          <div class="flex-1 min-w-0">
            <div class="truncate">{{ f.explain }}</div>
            <div class="text-xs text-muted-foreground truncate">{{ f.pluginName }}</div>
          </div>
          <div class="flex gap-1 shrink-0">
            <span
              v-for="c in f.cmds.slice(0, 2)"
              :key="c"
              class="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground"
              >{{ c }}</span
            >
          </div>
        </div>
        <div
          v-if="results.length === 0"
          class="px-4 py-8 text-center text-sm text-muted-foreground"
        >
          无匹配功能
        </div>
      </div>
    </div>
  </div>
</template>
