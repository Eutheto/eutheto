<script setup lang="ts">
import type { TooltipContentEmits, TooltipContentProps } from "reka-ui";
import type { HTMLAttributes } from "vue";
import { computed, useAttrs } from "vue";
import { TooltipContent, TooltipPortal, useForwardPropsEmits } from "reka-ui";
import { cn, withoutClass, withoutUndefined } from "../../../lib/utils";
defineOptions({ inheritAttrs: false });

const props = defineProps<TooltipContentProps & { class?: HTMLAttributes["class"] }>();
const emits = defineEmits<TooltipContentEmits>();
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
  <TooltipPortal>
    <TooltipContent
      v-bind="contentProps"
      :class="
        cn(
          'z-50 max-w-xs rounded-sm bg-ink px-3 py-2 text-xs font-semibold leading-relaxed text-canvas shadow-raised data-[state=closed]:animate-out data-[state=closed]:fade-out data-[state=open]:animate-in data-[state=open]:fade-in motion-reduce:animate-none',
          props.class,
        )
      "
    >
      <slot />
    </TooltipContent>
  </TooltipPortal>
</template>
