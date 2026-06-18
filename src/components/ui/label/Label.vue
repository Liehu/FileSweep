<script setup lang="ts">
import { Label } from "radix-vue";
import { cn } from "@/lib/utils";

interface Props {
  class?: string;
  for?: string;
}

const props = defineProps<Props>();
</script>

<template>
  <Label :for="props.for" :class="cn('text-sm font-medium leading-none peer-disabled:cursor-not-allowed peer-disabled:opacity-70', props.class)">
    <slot />
  </Label>
</template>
