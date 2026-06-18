<script setup lang="ts">
import { SelectLabel as RadixLabel } from "radix-vue";
import { cn } from "@/lib/utils";

const props = defineProps<{ class?: string }>();
</script>

<template>
  <RadixLabel :class="cn('px-2 py-1.5 text-sm font-semibold', props.class)"><slot /></RadixLabel>
</template>
