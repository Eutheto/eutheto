<script setup lang="ts">
import { computed, useId } from "vue";

import type { ValidationIssue } from "../../api/generated";
import { Badge, Card } from "../ui";
import { explanationMessage } from "./messages";
import type { ExplanationUiState } from "./types";

const props = withDefaults(
  defineProps<{
    issues: readonly ValidationIssue[];
    state: ExplanationUiState;
    selectedCode?: string | null;
  }>(),
  { selectedCode: null },
);

const emit = defineEmits<{
  selectIssue: [issue: ValidationIssue];
}>();

const headingId = useId();
const countText = computed(() =>
  explanationMessage("validation.issueCount", { count: props.issues.length }),
);
const showsIssues = computed(() => props.state === "ready" && props.issues.length > 0);
const stateText = computed(() => {
  if (props.state === "ready" || props.state === "empty") {
    return explanationMessage("validation.empty");
  }
  if (props.state === "loading") return explanationMessage("validation.loading");
  if (props.state === "stale") return explanationMessage("error.stale");
  if (props.state === "cancelled") return explanationMessage("error.cancelled");
  if (props.state === "inconclusive") return explanationMessage("error.inconclusive");
  if (props.state === "unavailable") return explanationMessage("error.unavailable");
  return explanationMessage("error.internal");
});

function severityVariant(issue: ValidationIssue): "danger" | "accent" | "neutral" {
  if (issue.severity === "error") return "danger";
  if (issue.severity === "warning") return "accent";
  return "neutral";
}
</script>

<template>
  <Card as="section" variant="surface" :aria-labelledby="headingId">
    <h2 :id="headingId" class="font-display text-lg font-bold text-ink">
      {{ explanationMessage("validation.heading") }}
    </h2>

    <p v-if="showsIssues" class="mt-2 text-sm text-muted" aria-live="polite" aria-atomic="true">
      {{ countText }}
    </p>
    <p v-else class="mt-2 text-sm text-muted" role="status" aria-live="polite">
      {{ stateText }}
    </p>

    <ul v-if="showsIssues" class="mt-4 space-y-3" aria-label="Validation issues">
      <li v-for="(issue, index) in issues" :key="`${issue.code}:${index}`">
        <button
          type="button"
          class="w-full rounded-md border border-line bg-raised p-3 text-left text-ink transition-colors hover:border-line-strong focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
          :aria-current="selectedCode === issue.code ? 'true' : undefined"
          :aria-label="`Select ${issue.severity} validation issue: ${issue.message}`"
          @click="emit('selectIssue', issue)"
        >
          <span class="flex flex-wrap items-center gap-2">
            <Badge :variant="severityVariant(issue)">{{ issue.severity }}</Badge>
            <span class="font-mono text-xs text-muted">{{ issue.code }}</span>
            <Badge v-if="selectedCode === issue.code" variant="outline">Selected</Badge>
          </span>
          <span class="mt-2 block text-sm font-semibold">{{ issue.message }}</span>
          <span v-if="issue.fieldPath" class="mt-1 block text-xs text-muted">
            Field: {{ issue.fieldPath }}
          </span>
          <span v-if="issue.resource" class="mt-1 block text-xs text-muted">
            Affected {{ issue.resource.type }}: {{ issue.resource.id }}
          </span>
        </button>
      </li>
    </ul>
  </Card>
</template>
