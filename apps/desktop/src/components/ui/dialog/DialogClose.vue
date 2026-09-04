<script setup lang="ts">
import type { DialogCloseProps } from "reka-ui";
import { computed } from "vue";
import { DialogClose, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<DialogCloseProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <DialogClose v-bind="forwarded">
    <slot />
  </DialogClose>
</template>
