<script setup lang="ts">
import type { TabsRootEmits, TabsRootProps } from "reka-ui";
import { computed } from "vue";
import { TabsRoot, useForwardPropsEmits } from "reka-ui";
import { withoutUndefined } from "../../../lib/utils";
defineOptions({ name: "UiTabs" });

const props = defineProps<TabsRootProps>();
const definedProps = computed(() => withoutUndefined(props));
const emits = defineEmits<TabsRootEmits>();
const forwarded = useForwardPropsEmits(definedProps, emits);
</script>

<template>
  <TabsRoot v-slot="slotProps" v-bind="forwarded">
    <slot v-bind="slotProps" />
  </TabsRoot>
</template>
