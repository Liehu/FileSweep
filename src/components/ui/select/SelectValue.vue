<script setup lang="ts">
import { SelectValue as RadixValue } from "radix-vue";

defineProps<{ placeholder?: string }>();
</script>

<template>
  <RadixValue :placeholder="placeholder" />
</template>
