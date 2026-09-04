<script lang="ts">
import type { VariantProps } from "class-variance-authority";
import { cva } from "class-variance-authority";

export const badgeVariants = cva(
  "inline-flex w-fit items-center rounded-full border px-3 py-1 font-body text-xs font-bold leading-none",
  {
    variants: {
      variant: {
        accent: "border-transparent bg-accent-soft text-accent-strong",
        neutral: "border-line bg-surface text-ink",
        danger: "border-danger bg-danger-soft text-danger-strong",
        outline: "border-line-strong bg-transparent text-ink",
      },
    },
    defaultVariants: {
      variant: "neutral",
    },
  },
);

export type BadgeVariants = VariantProps<typeof badgeVariants>;
</script>

<script setup lang="ts">
import type { HTMLAttributes } from "vue";
import { cn } from "../../../lib/utils";
defineOptions({ name: "UiBadge" });

interface BadgeProps {
  class?: HTMLAttributes["class"];
  variant?: BadgeVariants["variant"];
}

const props = withDefaults(defineProps<BadgeProps>(), {
  class: "",
  variant: "neutral",
});
</script>

<template>
  <span :class="cn(badgeVariants({ variant }), props.class)">
    <slot />
  </span>
</template>
