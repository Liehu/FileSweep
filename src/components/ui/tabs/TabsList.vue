<script setup lang="ts">
import { TabsList as RadixList } from "radix-vue";
import { cn } from "@/lib/utils";

const props = defineProps<{ class?: string }>();
</script>

<template>
  <RadixList :class="cn('inline-flex h-9 items-center justify-center rounded-lg bg-muted p-1 text-muted-foreground', props.class)">
    <slot />
  </RadixList>
</template>
