<script lang="ts">
import type { VariantProps } from "class-variance-authority";
import { cva } from "class-variance-authority";

export const cardVariants = cva("rounded-lg border p-5 font-body text-ink", {
  variants: {
    variant: {
      raised: "border-line bg-raised shadow-raised",
      surface: "border-line bg-surface",
      danger: "border-danger bg-danger-soft",
    },
  },
  defaultVariants: {
    variant: "raised",
  },
});

export type CardVariants = VariantProps<typeof cardVariants>;
</script>

<script setup lang="ts">
import type { HTMLAttributes } from "vue";
import { cn } from "../../../lib/utils";
defineOptions({ name: "UiCard" });

interface CardProps {
  as?: "article" | "div" | "section";
  class?: HTMLAttributes["class"];
  variant?: CardVariants["variant"];
}

const props = withDefaults(defineProps<CardProps>(), {
  as: "div",
  class: "",
  variant: "raised",
});
</script>

<template>
  <component :is="as" :class="cn(cardVariants({ variant }), props.class)">
    <slot />
  </component>
</template>
