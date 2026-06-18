<script setup lang="ts">
import { TabsRoot } from "radix-vue";

interface Props {
  modelValue?: string;
  defaultValue?: string;
}

defineProps<Props>();
const emit = defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <TabsRoot :model-value="modelValue" :default-value="defaultValue"
    @update:model-value="(v: string) => emit('update:modelValue', v)">
    <slot />
  </TabsRoot>
</template>
