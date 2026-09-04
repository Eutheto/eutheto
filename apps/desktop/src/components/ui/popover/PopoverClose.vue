<script setup lang="ts">
import type { PopoverCloseProps } from "reka-ui";
import { computed } from "vue";
import { PopoverClose, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<PopoverCloseProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <PopoverClose v-bind="forwarded">
    <slot />
  </PopoverClose>
</template>
