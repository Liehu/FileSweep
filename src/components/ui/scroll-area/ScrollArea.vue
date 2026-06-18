<script setup lang="ts">
import { ScrollAreaRoot, ScrollAreaScrollbar, ScrollAreaThumb, ScrollAreaViewport, ScrollAreaCorner } from "radix-vue";
import { cn } from "@/lib/utils";

interface Props {
  class?: string;
  orientation?: "vertical" | "horizontal";
}

const props = withDefaults(defineProps<Props>(), { orientation: "vertical" });
</script>

<template>
  <ScrollAreaRoot :class="cn('relative overflow-hidden', props.class)">
    <ScrollAreaViewport class="h-full w-full rounded-[inherit]">
      <slot />
    </ScrollAreaViewport>
    <ScrollAreaScrollbar
      :orientation="orientation"
      class="flex touch-none select-none transition-colors"
      :class="orientation === 'vertical' ? 'h-full w-2.5 border-l border-l-transparent p-[1px]' : 'h-2.5 flex-col border-t border-t-transparent p-[1px]'"
    >
      <ScrollAreaThumb class="relative flex-1 rounded-full bg-border" />
    </ScrollAreaScrollbar>
    <ScrollAreaCorner />
  </ScrollAreaRoot>
</template>
