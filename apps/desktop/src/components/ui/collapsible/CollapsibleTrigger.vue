<script setup lang="ts">
import type { CollapsibleTriggerProps } from "reka-ui";
import { computed } from "vue";
import { CollapsibleTrigger, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<CollapsibleTriggerProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <CollapsibleTrigger v-bind="forwarded">
    <slot />
  </CollapsibleTrigger>
</template>
