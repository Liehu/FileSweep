<script setup lang="ts">
import { ProgressRoot, ProgressIndicator } from "radix-vue";
import { cn } from "@/lib/utils";

interface Props {
  modelValue?: number;
  max?: number;
  class?: string;
}

const props = withDefaults(defineProps<Props>(), { max: 100 });
</script>

<template>
  <ProgressRoot
    :model-value="modelValue"
    :max="max"
    :class="cn('relative h-2 w-full overflow-hidden rounded-full bg-primary/20', props.class)"
  >
    <ProgressIndicator
      class="h-full w-full flex-1 bg-primary transition-all"
      :style="{ transform: `translateX(-${100 - (modelValue != null ? (modelValue / max) * 100 : 0)}%)` }"
    />
  </ProgressRoot>
</template>
