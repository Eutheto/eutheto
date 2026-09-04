<script setup lang="ts">
import type { CollapsibleContentEmits, CollapsibleContentProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed } from "vue";
import { CollapsibleContent, useForwardProps } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";

const props = defineProps<CollapsibleContentProps & { class?: HTMLAttributes["class"] }>();
const emits = defineEmits<CollapsibleContentEmits>();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardProps(delegatedProps);
</script>

<template>
  <CollapsibleContent
    v-bind="forwarded"
    :class="cn('overflow-hidden', props.class)"
    @content-found="emits('contentFound')"
  >
    <slot />
  </CollapsibleContent>
</template>
