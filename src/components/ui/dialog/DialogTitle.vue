<script setup lang="ts">
import { DialogTitle as RadixTitle } from "radix-vue";
import { cn } from "@/lib/utils";

const props = defineProps<{ class?: string }>();
</script>

<template>
  <RadixTitle :class="cn('text-lg font-semibold leading-none tracking-tight', props.class)">
    <slot />
  </RadixTitle>
</template>
