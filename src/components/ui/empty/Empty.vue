<script setup lang="ts">
import { cn } from "@/lib/utils";
import { Inbox } from "lucide-vue-next";

interface Props {
  icon?: any;
  message?: string;
  class?: string;
}

const props = withDefaults(defineProps<Props>(), {
  message: "暂无数据",
});
</script>

<template>
  <div :class="cn('flex flex-col items-center justify-center py-12 text-muted-foreground', props.class)">
    <component :is="icon || Inbox" class="h-12 w-12 mb-3 opacity-50" />
    <p class="text-sm">{{ message }}</p>
    <slot />
  </div>
</template>
