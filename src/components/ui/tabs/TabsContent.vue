<script setup lang="ts">
import { TabsContent as RadixContent } from "radix-vue";
import { cn } from "@/lib/utils";

interface Props {
  value: string;
  class?: string;
}

const props = defineProps<Props>();
</script>

<template>
  <RadixContent :value="value" :class="cn(
    'mt-2 ring-offset-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2',
    props.class)">
    <slot />
  </RadixContent>
</template>
