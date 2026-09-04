<script setup lang="ts">
import type { DialogRootEmits, DialogRootProps } from "reka-ui";
import { computed } from "vue";
import { DialogRoot, useForwardPropsEmits } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";
defineOptions({ name: "UiDialog" });

type DialogProps = Omit<DialogRootProps, "defaultOpen" | "modal" | "open" | "unmountOnHide"> & {
  defaultOpen?: DialogRootProps["defaultOpen"] | undefined;
  modal?: DialogRootProps["modal"] | undefined;
  open?: DialogRootProps["open"] | undefined;
  unmountOnHide?: DialogRootProps["unmountOnHide"] | undefined;
};

const props = withDefaults(defineProps<DialogProps>(), {
  defaultOpen: undefined,
  modal: undefined,
  open: undefined,
  unmountOnHide: undefined,
});
const definedProps = computed(() => withoutUndefined(props));
const emits = defineEmits<DialogRootEmits>();
const forwarded = useForwardPropsEmits(definedProps, emits);
</script>

<template>
  <DialogRoot v-slot="slotProps" v-bind="forwarded">
    <slot v-bind="slotProps" />
  </DialogRoot>
</template>
