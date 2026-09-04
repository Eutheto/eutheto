<script setup lang="ts">
import type { TooltipProviderProps } from "reka-ui";
import { computed } from "vue";
import { TooltipProvider, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<TooltipProviderProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <TooltipProvider v-bind="forwarded">
    <slot />
  </TooltipProvider>
</template>
