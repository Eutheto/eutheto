<script setup lang="ts">
import type { TabsContentProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed } from "vue";
import { TabsContent, useForwardProps } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";

const props = defineProps<TabsContentProps & { class?: HTMLAttributes["class"] }>();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardProps(delegatedProps);
</script>

<template>
  <TabsContent
    v-bind="forwarded"
    :class="
      cn(
        'mt-4 outline-none focus-visible:ring-3 focus-visible:ring-focus focus-visible:ring-offset-2',
        props.class,
      )
    "
  >
    <slot />
  </TabsContent>
</template>
