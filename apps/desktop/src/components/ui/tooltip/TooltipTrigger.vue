<script setup lang="ts">
import type { TooltipTriggerProps } from "reka-ui";
import { computed } from "vue";
import { TooltipTrigger, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<TooltipTriggerProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <TooltipTrigger v-bind="forwarded">
    <slot />
  </TooltipTrigger>
</template>
