<script setup lang="ts">
import { computed, useId } from "vue";

import type { InfeasibilityEvidenceV1 } from "../../api/generated";
import { Badge, Card } from "../ui";
import { explanationMessage } from "./messages";
import type { ExplanationUiState } from "./types";

const props = defineProps<{
  evidence: InfeasibilityEvidenceV1 | null;
  state: ExplanationUiState;
  ruleLabels: Readonly<Record<string, string>>;
}>();

const emit = defineEmits<{
  inspect: [ruleId: string];
  edit: [ruleId: string];
  relaxInCopy: [groupId: string];
  paraphrase: [];
  exportDiagnostic: [];
  cancel: [];
}>();

const headingId = useId();
const conflict = computed(() =>
  props.state === "ready" && props.evidence?.type === "conflict" ? props.evidence : null,
);
const unavailableEvidence = computed(() =>
  props.state === "ready" && props.evidence?.type === "unavailable" ? props.evidence : null,
);
const headingText = computed(() => {
  if (conflict.value) return explanationMessage("conflict.heading");
  if (unavailableEvidence.value) return "Conflict evidence unavailable";
  return "Conflict evidence";
});
const stateText = computed(() => {
  if (props.state === "loading") return "Preparing mapped conflict evidence…";
  if (props.state === "stale") return explanationMessage("error.stale");
  if (props.state === "cancelled") return "Conflict analysis was cancelled.";
  if (props.state === "inconclusive") return explanationMessage("error.inconclusive");
  if (props.state === "unavailable") return explanationMessage("error.unavailable");
  if (props.state === "internalFailure") return explanationMessage("error.internal");
  return explanationMessage("conflict.empty");
});

const unavailableReason = computed(() => {
  const reason = unavailableEvidence.value?.reason;
  if (reason === "assumptionsUnavailable") {
    return "This model does not expose assumptions that can be mapped to required rules.";
  }
  if (reason === "foundationalInfeasibility") {
    return "Infeasibility comes from foundational constraints outside the mapped required rules.";
  }
  if (reason === "conflictNotReturned") {
    return "The backend did not return usable conflict evidence.";
  }
  if (reason === "invalidAssumptionCore") {
    return "The returned conflict evidence was invalid and was not shown.";
  }
  return null;
});

function formatCount(value: number): string {
  return new Intl.NumberFormat().format(value);
}

const shrinkText = computed(() => {
  const shrink = conflict.value?.shrink;
  if (!shrink) return "";

  const retained = `${formatCount(shrink.remainingGroupCount)} of ${formatCount(shrink.initialGroupCount)} groups remain.`;
  const trials = formatCount(shrink.attemptedTrials);
  if (shrink.stopReason === "completed") {
    return `${retained} Shrinking completed after ${trials} of ${formatCount(shrink.maxTrials)} permitted trials.`;
  }
  if (shrink.stopReason === "notAttempted") {
    return `${retained} Conflict shrinking was not attempted.`;
  }
  if (shrink.stopReason === "trialLimit") {
    return `${retained} Conflict shrinking stopped at the trial limit after ${trials} trials.`;
  }
  if (shrink.stopReason === "budgetExpired") {
    return `${retained} Conflict shrinking stopped because its diagnostic time budget expired after ${trials} trials.`;
  }
  if (shrink.stopReason === "cancelled") {
    return `${retained} Conflict shrinking was cancelled after ${trials} trials.`;
  }
  return `${retained} Conflict shrinking stopped because a trial was inconclusive after ${trials} trials.`;
});

function ruleLabel(ruleId: string): string {
  return props.ruleLabels[ruleId] ?? `Required rule ${ruleId}`;
}
</script>

<template>
  <Card
    as="article"
    :variant="state === 'internalFailure' ? 'danger' : 'surface'"
    :aria-labelledby="headingId"
  >
    <h2 :id="headingId" class="font-display text-lg font-bold text-ink">
      {{ headingText }}
    </h2>

    <template v-if="conflict">
      <div class="mt-3 flex flex-wrap items-center gap-2">
        <Badge :variant="conflict.minimality === 'provenMinimal' ? 'neutral' : 'accent'">
          {{
            conflict.minimality === "provenMinimal"
              ? explanationMessage("conflict.minimal")
              : explanationMessage("conflict.sufficient")
          }}
        </Badge>
        <span class="text-sm text-muted">
          {{
            conflict.minimality === "provenMinimal"
              ? "Every remaining group was proven necessary under this grouping."
              : "This set is enough to prove infeasibility; other conflicts may also exist."
          }}
        </span>
      </div>

      <p class="mt-3 text-sm text-muted">{{ shrinkText }}</p>

      <ol class="mt-4 space-y-4" aria-label="Mapped conflict groups">
        <li
          v-for="group in conflict.groups"
          :key="group.groupId"
          class="rounded-md border border-line bg-raised p-4"
        >
          <h3 class="font-semibold text-ink">Conflict group {{ group.groupId }}</h3>
          <ul class="mt-2 space-y-2" :aria-label="`Required rules in ${group.groupId}`">
            <li
              v-for="ruleId in group.requiredRules"
              :key="ruleId"
              class="flex flex-wrap items-center justify-between gap-2 border-t border-line pt-2 first:border-t-0 first:pt-0"
            >
              <span class="text-sm text-ink">{{ ruleLabel(ruleId) }}</span>
              <span class="flex flex-wrap gap-2">
                <button
                  type="button"
                  class="rounded-md border border-line-strong px-3 py-1 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                  @click="emit('inspect', ruleId)"
                >
                  {{ explanationMessage("action.inspect") }}
                  <span class="sr-only"> {{ ruleLabel(ruleId) }}</span>
                </button>
                <button
                  type="button"
                  class="rounded-md border border-line-strong px-3 py-1 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                  @click="emit('edit', ruleId)"
                >
                  {{ explanationMessage("action.edit") }}
                  <span class="sr-only"> {{ ruleLabel(ruleId) }}</span>
                </button>
              </span>
            </li>
          </ul>
          <button
            type="button"
            class="mt-3 rounded-md bg-accent px-3 py-2 text-sm font-bold text-raised focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
            @click="emit('relaxInCopy', group.groupId)"
          >
            {{ explanationMessage("action.relaxCopy") }}
            <span class="sr-only"> for conflict group {{ group.groupId }}</span>
          </button>
        </li>
      </ol>

      <p class="mt-4 text-sm text-muted">
        {{ explanationMessage("conflict.paraphrase") }}
      </p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          class="rounded-md border border-line-strong px-3 py-2 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
          @click="emit('paraphrase')"
        >
          {{ explanationMessage("action.paraphrase") }}
        </button>
        <button
          type="button"
          class="rounded-md border border-line-strong px-3 py-2 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
          @click="emit('exportDiagnostic')"
        >
          {{ explanationMessage("conflict.diagnosticExport") }}
        </button>
      </div>
    </template>

    <div v-else-if="unavailableReason" class="mt-3" role="status">
      <Badge variant="outline">Unavailable</Badge>
      <p class="mt-2 text-sm text-muted">{{ unavailableReason }}</p>
      <p class="mt-1 text-sm text-muted">No required rules were guessed or inferred.</p>
      <button
        type="button"
        class="mt-3 rounded-md border border-line-strong px-3 py-2 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
        @click="emit('exportDiagnostic')"
      >
        {{ explanationMessage("conflict.diagnosticExport") }}
      </button>
    </div>

    <div v-else class="mt-3" :role="state === 'internalFailure' ? 'alert' : 'status'">
      <p class="text-sm text-muted">{{ stateText }}</p>
      <button
        v-if="state === 'loading'"
        type="button"
        class="mt-3 rounded-md border border-line-strong px-3 py-2 text-sm font-semibold text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
        @click="emit('cancel')"
      >
        {{ explanationMessage("action.cancel") }}
      </button>
    </div>
  </Card>
</template>
