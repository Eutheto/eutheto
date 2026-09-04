<script setup lang="ts">
import type { PopoverTriggerProps } from "reka-ui";
import { computed } from "vue";
import { PopoverTrigger, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<PopoverTriggerProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <PopoverTrigger v-bind="forwarded">
    <slot />
  </PopoverTrigger>
</template>
