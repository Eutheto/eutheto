<script setup lang="ts">
import type { TooltipRootEmits, TooltipRootProps } from "reka-ui";
import { computed } from "vue";
import { TooltipRoot, useForwardPropsEmits } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";
defineOptions({ name: "UiTooltip" });

type TooltipProps = Omit<
  TooltipRootProps,
  | "defaultOpen"
  | "disableClosingTrigger"
  | "disableHoverableContent"
  | "disabled"
  | "ignoreNonKeyboardFocus"
  | "open"
> & {
  defaultOpen?: TooltipRootProps["defaultOpen"] | undefined;
  disableClosingTrigger?: TooltipRootProps["disableClosingTrigger"] | undefined;
  disableHoverableContent?: TooltipRootProps["disableHoverableContent"] | undefined;
  disabled?: TooltipRootProps["disabled"] | undefined;
  ignoreNonKeyboardFocus?: TooltipRootProps["ignoreNonKeyboardFocus"] | undefined;
  open?: TooltipRootProps["open"] | undefined;
};

const props = withDefaults(defineProps<TooltipProps>(), {
  defaultOpen: undefined,
  disableClosingTrigger: undefined,
  disableHoverableContent: undefined,
  disabled: undefined,
  ignoreNonKeyboardFocus: undefined,
  open: undefined,
});
const definedProps = computed(() => withoutUndefined(props));
const emits = defineEmits<TooltipRootEmits>();
const forwarded = useForwardPropsEmits(definedProps, emits);
</script>

<template>
  <TooltipRoot v-slot="slotProps" v-bind="forwarded">
    <slot v-bind="slotProps" />
  </TooltipRoot>
</template>
