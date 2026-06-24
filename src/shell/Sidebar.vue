<script setup lang="ts">
import { computed } from "vue";
import { useRouter, useRoute } from "vue-router";
import { getPlugins, type NavGroup } from "@/lib/plugin";
import { getIcon } from "./iconMap";
import { Separator } from "@/components/ui/separator";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";

const props = defineProps<{
  /** 动态分类组（由 AppShell 从 settingsStore.rules 注入） */
  categoryNav?: { title: string; items: { label: string; route: string; query: Record<string, string> }[] };
  /** badge 数据，key = "pluginId:itemLabel"，值显示为角标 */
  badges?: Record<string, number | string | undefined>;
}>();

const router = useRouter();
const route = useRoute();

const staticGroups = computed(() => {
  const groups: NavGroup[] = [];
  for (const plugin of getPlugins()) {
    if (plugin.navGroups) groups.push(...plugin.navGroups);
  }
  return groups;
});

// 完整导航组：文件组 + 分类组 + 其余插件组
// filesweep 提供 [文件, 工具] 两组，分类组插在它们之间；
// 其他插件（如 appmover）的导航组追加到末尾。
const allGroups = computed(() => {
  const result: NavGroup[] = [];
  const groups = staticGroups.value;
  // filesweep 的"文件"组（第一组）
  if (groups[0]) result.push(groups[0]);
  // 分类组（动态，插在文件与工具之间）
  if (props.categoryNav) {
    result.push({
      title: props.categoryNav.title,
      items: props.categoryNav.items.map((i) => ({
        label: i.label,
        icon: "FolderOpen",
        route: i.route,
        query: i.query,
      })),
    });
  }
  // filesweep 的"工具"组（第二组）
  if (groups[1]) result.push(groups[1]);
  // 其余插件的导航组（index >= 2），全部追加到末尾
  for (let i = 2; i < groups.length; i++) {
    result.push(groups[i]);
  }
  return result;
});

function isActive(path: string, query?: Record<string, string>) {
  if (route.path !== path) return false;
  if (!query || Object.keys(query).length === 0) {
    // 无 query 的项，要求当前也无相关 query（避免「全部文件」与「重复文件」同时高亮）
    if (path === "/files") {
      return !route.query.dup && !route.query.mv && !route.query.cat;
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
              <Badge
                v-if="badges && badges[item.label]"
                variant="secondary"
                class="ml-auto text-[10px] px-1"
              >
                {{ badges[item.label] }}
              </Badge>
            </button>
          </div>
        </div>
        <Separator v-if="gi < allGroups.length - 1" class="my-1" />
      </template>
    </ScrollArea>
  </aside>
</template>
