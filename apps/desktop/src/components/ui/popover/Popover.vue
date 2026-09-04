<script setup lang="ts">
import type { PopoverRootEmits, PopoverRootProps } from "reka-ui";
import { computed } from "vue";
import { PopoverRoot, useForwardPropsEmits } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";
defineOptions({ name: "UiPopover" });

type PopoverProps = Omit<PopoverRootProps, "defaultOpen" | "modal" | "open"> & {
  defaultOpen?: PopoverRootProps["defaultOpen"] | undefined;
  modal?: PopoverRootProps["modal"] | undefined;
  open?: PopoverRootProps["open"] | undefined;
};

const props = withDefaults(defineProps<PopoverProps>(), {
  defaultOpen: undefined,
  modal: undefined,
  open: undefined,
});
const definedProps = computed(() => withoutUndefined(props));
const emits = defineEmits<PopoverRootEmits>();
const forwarded = useForwardPropsEmits(definedProps, emits);
</script>

<template>
  <PopoverRoot v-slot="slotProps" v-bind="forwarded">
    <slot v-bind="slotProps" />
  </PopoverRoot>
</template>
