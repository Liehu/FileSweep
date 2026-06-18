<script setup lang="ts">
import { SelectContent as RadixContent, SelectPortal, SelectViewport } from "radix-vue";
import { cn } from "@/lib/utils";

const props = withDefaults(defineProps<{ class?: string; position?: "popper" | "item-aligned" }>(), { position: "popper" });
</script>

<template>
  <SelectPortal>
    <RadixContent :position="position" :class="cn(
      'relative z-50 max-h-96 min-w-[8rem] overflow-hidden rounded-md border text-popover-foreground shadow-md data-[state=open]:animate-in data-[state=closed]:animate-out data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0 data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95',
      position === 'popper' && 'data-[side=bottom]:translate-y-1 data-[side=left]:-translate-x-1 data-[side=right]:translate-x-1 data-[side=top]:-translate-y-1',
      props.class)"
      style="background-color: hsl(0 0% 100%); opacity: 1;">
      <SelectViewport :class="cn('p-1', position === 'popper' && 'h-[var(--radix-select-trigger-height)] w-full min-w-[var(--radix-select-trigger-width)]')">
        <slot />
      </SelectViewport>
    </RadixContent>
  </SelectPortal>
</template>
