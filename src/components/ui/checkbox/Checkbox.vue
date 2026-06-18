<script setup lang="ts">
import { CheckboxRoot, CheckboxIndicator } from "radix-vue";
import { cn } from "@/lib/utils";
import { Check, Minus } from "lucide-vue-next";
import { computed } from "vue";

interface Props {
  modelValue?: boolean | "indeterminate";
  disabled?: boolean;
  class?: string;
}

const props = defineProps<Props>();
const emit = defineEmits<{ "update:modelValue": [value: boolean] }>();

const isChecked = computed(() => props.modelValue === true);
const isIndeterminate = computed(() => props.modelValue === "indeterminate");

function onCheckedChange(val: boolean | "indeterminate") {
  emit("update:modelValue", val === "indeterminate" ? true : val);
}
</script>

<template>
  <CheckboxRoot
    :checked="isIndeterminate ? 'indeterminate' : modelValue"
    :disabled="disabled"
    :class="cn(
      'peer h-4 w-4 shrink-0 rounded-sm border border-primary shadow focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground',
      props.class
    )"
    @update:checked="onCheckedChange"
  >
    <CheckboxIndicator class="flex items-center justify-center text-current">
      <Minus v-if="isIndeterminate" class="h-3.5 w-3.5" />
      <Check v-else class="h-3.5 w-3.5" />
    </CheckboxIndicator>
  </CheckboxRoot>
</template>
