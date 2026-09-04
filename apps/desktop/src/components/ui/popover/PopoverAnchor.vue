<script setup lang="ts">
import type { PopoverAnchorProps } from "reka-ui";
import { computed } from "vue";
import { PopoverAnchor, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<PopoverAnchorProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <PopoverAnchor v-bind="forwarded">
    <slot />
  </PopoverAnchor>
</template>
