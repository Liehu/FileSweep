<script setup lang="ts">
import { DialogDescription as RadixDesc } from "radix-vue";
import { cn } from "@/lib/utils";

const props = defineProps<{ class?: string }>();
</script>

<template>
  <RadixDesc :class="cn('text-sm text-muted-foreground', props.class)">
    <slot />
  </RadixDesc>
</template>
