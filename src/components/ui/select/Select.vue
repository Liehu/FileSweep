<script setup lang="ts">
import { SelectRoot } from "radix-vue";

interface Props {
  modelValue?: string;
  defaultValue?: string;
  disabled?: boolean;
}

defineProps<Props>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <SelectRoot :model-value="modelValue" :default-value="defaultValue" :disabled="disabled"
    @update:model-value="(v: string) => emit('update:modelValue', v)">
    <slot />
  </SelectRoot>
</template>
