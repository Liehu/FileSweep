<script setup lang="ts">
import { SliderRoot, SliderTrack, SliderRange, SliderThumb } from "radix-vue";
import { cn } from "@/lib/utils";

interface Props {
  modelValue?: number[];
  min?: number;
  max?: number;
  step?: number;
  disabled?: boolean;
  class?: string;
}

const props = withDefaults(defineProps<Props>(), {
  modelValue: () => [0],
  min: 0,
  max: 100,
  step: 1,
});

const emit = defineEmits<{ "update:modelValue": [value: number[]] }>();
</script>

<template>
  <SliderRoot
    :model-value="modelValue"
    :min="min"
    :max="max"
    :step="step"
    :disabled="disabled"
    :class="cn('relative flex w-full touch-none select-none items-center', props.class)"
    @update:model-value="(v: number[] | undefined) => emit('update:modelValue', v ?? [])"
  >
    <SliderTrack class="relative h-1.5 w-full grow overflow-hidden rounded-full bg-primary/20">
      <SliderRange class="absolute h-full bg-primary" />
    </SliderTrack>
    <SliderThumb
      v-for="(_, i) in modelValue"
      :key="i"
      class="block h-4 w-4 rounded-full border border-primary/50 bg-background shadow transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50"
    />
  </SliderRoot>
</template>
