<script setup lang="ts">
import type { TabsTriggerProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed } from "vue";
import { TabsTrigger, useForwardProps } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";

const props = defineProps<TabsTriggerProps & { class?: HTMLAttributes["class"] }>();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardProps(delegatedProps);
</script>

<template>
  <TabsTrigger
    v-bind="forwarded"
    :class="
      cn(
        'min-h-0! rounded-sm border-transparent! bg-transparent! px-3! py-2! text-sm font-bold text-muted! shadow-none! hover:border-transparent! hover:bg-accent-soft! hover:text-accent-strong! data-[state=active]:bg-raised! data-[state=active]:text-ink! data-[state=active]:shadow-sm!',
        props.class,
      )
    "
  >
    <slot />
  </TabsTrigger>
</template>
