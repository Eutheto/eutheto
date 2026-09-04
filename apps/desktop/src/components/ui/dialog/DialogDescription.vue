<script setup lang="ts">
import type { DialogDescriptionProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed } from "vue";
import { DialogDescription, useForwardProps } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";

const props = defineProps<DialogDescriptionProps & { class?: HTMLAttributes["class"] }>();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardProps(delegatedProps);
</script>

<template>
  <DialogDescription
    v-bind="forwarded"
    :class="cn('text-sm leading-relaxed text-muted', props.class)"
  >
    <slot />
  </DialogDescription>
</template>
