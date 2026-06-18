<script setup lang="ts">
import { SelectTrigger as RadixTrigger, SelectValue as RadixValue } from "radix-vue";
import { cn } from "@/lib/utils";
import { ChevronDown } from "lucide-vue-next";

const props = defineProps<{ class?: string; placeholder?: string }>();
</script>

<template>
  <RadixTrigger :class="cn(
    'flex h-9 w-full items-center justify-between whitespace-nowrap rounded-md border border-input bg-transparent px-3 py-2 text-sm shadow-sm placeholder:text-muted-foreground focus:outline-none focus:ring-1 focus:ring-ring disabled:cursor-not-allowed disabled:opacity-50',
    props.class)">
    <slot><RadixValue :placeholder="placeholder" /></slot>
    <ChevronDown class="h-4 w-4 opacity-50" />
  </RadixTrigger>
</template>
