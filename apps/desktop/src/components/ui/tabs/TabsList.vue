<script setup lang="ts">
import type { TabsListProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed } from "vue";
import { TabsList, useForwardProps } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";

const props = defineProps<TabsListProps & { class?: HTMLAttributes["class"] }>();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardProps(delegatedProps);
</script>

<template>
  <TabsList
    v-bind="forwarded"
    :class="
      cn('inline-flex items-center gap-1 rounded-md border border-line bg-surface p-1', props.class)
    "
  >
    <slot />
  </TabsList>
</template>
