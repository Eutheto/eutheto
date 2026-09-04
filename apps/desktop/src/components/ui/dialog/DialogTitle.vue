<script setup lang="ts">
import type { DialogTitleProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed } from "vue";
import { DialogTitle, useForwardProps } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";

const props = defineProps<DialogTitleProps & { class?: HTMLAttributes["class"] }>();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardProps(delegatedProps);
</script>

<template>
  <DialogTitle
    v-bind="forwarded"
    :class="cn('font-display text-2xl! font-semibold text-ink', props.class)"
  >
    <slot />
  </DialogTitle>
</template>
