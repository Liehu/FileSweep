<script setup lang="ts">
import { DialogRoot } from "radix-vue";

interface Props {
  open?: boolean;
  defaultOpen?: boolean;
}

defineProps<Props>();
const emit = defineEmits<{ "update:open": [value: boolean] }>();
</script>

<template>
  <DialogRoot :open="open" :default-open="defaultOpen" @update:open="(v: boolean) => emit('update:open', v)">
    <slot />
  </DialogRoot>
</template>
