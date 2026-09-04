<script setup lang="ts">
import type { DialogTriggerProps } from "reka-ui";
import { computed } from "vue";
import { DialogTrigger, useForwardProps } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";

const props = defineProps<DialogTriggerProps>();
const definedProps = computed(() => withoutUndefined(props));
const forwarded = useForwardProps(definedProps);
</script>

<template>
  <DialogTrigger v-bind="forwarded">
    <slot />
  </DialogTrigger>
</template>
