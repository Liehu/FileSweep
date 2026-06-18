<script setup lang="ts">
import { SelectGroup as RadixGroup } from "radix-vue";
import { cn } from "@/lib/utils";

const props = defineProps<{ class?: string }>();
</script>

<template>
  <RadixGroup :class="cn('p-1 w-full', props.class)"><slot /></RadixGroup>
</template>
