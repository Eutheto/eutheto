<script setup lang="ts">
import type { CollapsibleRootEmits, CollapsibleRootProps } from "reka-ui";
import { computed } from "vue";
import { CollapsibleRoot, useForwardPropsEmits } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";
defineOptions({ name: "UiCollapsible" });

type CollapsibleProps = Omit<
  CollapsibleRootProps,
  "asChild" | "defaultOpen" | "disabled" | "open" | "unmountOnHide"
> & {
  asChild?: CollapsibleRootProps["asChild"] | undefined;
  defaultOpen?: CollapsibleRootProps["defaultOpen"] | undefined;
  disabled?: CollapsibleRootProps["disabled"] | undefined;
  open?: CollapsibleRootProps["open"] | undefined;
  unmountOnHide?: CollapsibleRootProps["unmountOnHide"] | undefined;
};

const props = withDefaults(defineProps<CollapsibleProps>(), {
  asChild: undefined,
  defaultOpen: undefined,
  disabled: undefined,
  open: undefined,
  unmountOnHide: undefined,
});
const definedProps = computed(() => withoutUndefined(props));
const emits = defineEmits<CollapsibleRootEmits>();
const forwarded = useForwardPropsEmits(definedProps, emits);
</script>

<template>
  <CollapsibleRoot v-slot="slotProps" v-bind="forwarded">
    <slot v-bind="slotProps" />
  </CollapsibleRoot>
</template>
