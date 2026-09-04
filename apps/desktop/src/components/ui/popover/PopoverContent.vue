<script setup lang="ts">
import type { PopoverContentEmits, PopoverContentProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed, useAttrs } from "vue";
import { PopoverContent, PopoverPortal, useForwardPropsEmits } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";
defineOptions({ inheritAttrs: false });

const props = defineProps<PopoverContentProps & { class?: HTMLAttributes["class"] }>();
const emits = defineEmits<PopoverContentEmits>();
const attrs = useAttrs();
const delegatedProps = computed(() => {
  return withoutUndefined(withoutClass(props));
});
const forwarded = useForwardPropsEmits(delegatedProps, emits);
const contentProps = computed(() =>
  withoutUndefined({
    ...forwarded.value,
    ...attrs,
  }),
);
</script>

<template>
  <PopoverPortal>
    <PopoverContent
      v-bind="contentProps"
      :class="
        cn(
          'z-50 w-80 rounded-md border border-line bg-raised p-4 text-ink shadow-raised data-[state=closed]:animate-out data-[state=closed]:fade-out data-[state=closed]:zoom-out-95 data-[state=open]:animate-in data-[state=open]:fade-in data-[state=open]:zoom-in-95 motion-reduce:animate-none',
          props.class,
        )
      "
    >
      <slot />
    </PopoverContent>
  </PopoverPortal>
</template>
