<script setup lang="ts">
import { computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import { getPlugins, type NavGroup } from "@/lib/plugin";
import { getIcon } from "./iconMap";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";

const router = useRouter();
const route = useRoute();

// 收集所有插件的导航组（功能菜单）
const allGroups = computed(() => {
  const groups: NavGroup[] = [];
  for (const plugin of getPlugins()) {
    if (plugin.navGroups) groups.push(...plugin.navGroups);
  }
  return groups;
});

function isActive(path: string, query?: Record<string, string>) {
  if (route.path !== path) return false;
  if (!query || Object.keys(query).length === 0) {
    // 无 query 的项，要求当前也无相关 query（避免「全部文件」与「重复文件」同时高亮）
    if (path === "/files") {
      return !route.query.dup && !route.query.mv && !route.query.cat && !route.query.dtype;
    }
    return true;
  }
  return Object.entries(query).every(([k, v]) => route.query[k] === v);
}

function navigateTo(path: string, query?: Record<string, string>) {
  if (query && Object.keys(query).length > 0) {
    router.push({ path, query });
  } else {
    router.push(path);
  }
}
</script>

<template>
  <aside
    :class="[
      'flex flex-col border-r bg-card transition-all duration-200',
      $attrs.class,
    ]"
  >
    <ScrollArea class="flex-1">
      <template v-for="(group, gi) in allGroups" :key="group.title">
        <div class="p-3">
          <p class="text-xs text-muted-foreground mb-2 px-1">{{ group.title }}</p>
          <div class="space-y-0.5">
            <button
              v-for="item in group.items"
              :key="item.label"
              :class="[
                'flex items-center gap-2 w-full px-2 py-1.5 rounded-md text-sm transition-colors',
                isActive(item.route, item.query)
                  ? 'bg-primary text-primary-foreground'
                  : 'hover:bg-accent text-foreground',
              ]"
              @click="navigateTo(item.route, item.query)"
            >
              <component :is="getIcon(item.icon)" class="h-4 w-4" />
              <span>{{ item.label }}</span>
            </button>
          </div>
        </div>
        <Separator v-if="gi < allGroups.length - 1" class="my-1" />
      </template>
    </ScrollArea>
  </aside>
</template>
