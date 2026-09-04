<script setup lang="ts">
import { computed } from "vue";
import type { ScoreContributionV1, ScoreVector } from "../../api/generated";
import { Badge, Card } from "../ui";
import { explanationMessage } from "./messages";
import type { ExplanationUiState } from "./types";

interface Props {
  readonly score: ScoreVector | null;
  readonly contributions: readonly ScoreContributionV1[];
  readonly state: ExplanationUiState;
  readonly levelLabels?: Readonly<Record<string, string>>;
  readonly categoryLabels?: Readonly<Record<string, string>>;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  selectContribution: [contribution: ScoreContributionV1];
}>();

function formatInteger(value: string | number): string {
  try {
    return new Intl.NumberFormat().format(typeof value === "string" ? BigInt(value) : value);
  } catch {
    return String(value);
  }
}

function formatSigned(value: string | number): string {
  const text = String(value);
  if (text.startsWith("-")) return `−${formatInteger(text.slice(1))}`;
  if (text === "0") return "0";
  return `+${formatInteger(value)}`;
}

function isZero(value: string | number): boolean {
  return String(value) === "0";
}

function directionLabel(direction: "minimize" | "maximize"): string {
  return direction === "minimize" ? "Minimize — lower is better" : "Maximize — higher is better";
}

const stateCopy = computed(() => {
  switch (props.state) {
    case "ready":
      return props.score ? null : explanationMessage("score.empty");
    case "empty":
      return explanationMessage("score.empty");
    case "loading":
      return "Verified scores are being prepared. Return to this result when verification finishes.";
    case "stale":
      return "These scores belong to an earlier scenario revision. Refresh the result before using them.";
    case "cancelled":
      return "Score verification was cancelled. Run the solve again to produce a verified breakdown.";
    case "inconclusive":
      return "The available evidence does not establish a verified score breakdown.";
    case "unavailable":
      return "A verified score breakdown is unavailable. Try the solve again when the backend is available.";
    case "internalFailure":
      return explanationMessage("error.internal");
  }
});

const stateRole = computed(() =>
  props.state === "stale" || props.state === "internalFailure" ? "alert" : "status",
);
</script>

<template>
  <Card as="section" aria-labelledby="score-breakdown-heading">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h2 id="score-breakdown-heading" class="font-display text-xl font-semibold text-ink">
        {{ explanationMessage("score.heading") }}
      </h2>
      <Badge
        v-if="score && state === 'ready'"
        :variant="isZero(score.feasibility) ? 'accent' : 'danger'"
      >
        <span aria-hidden="true">{{ isZero(score.feasibility) ? "✓" : "✕" }}</span>
        {{
          isZero(score.feasibility)
            ? explanationMessage("score.requiredPassed")
            : explanationMessage("score.requiredFailed")
        }}
      </Badge>
    </div>

    <div
      v-if="stateCopy"
      class="mt-4 rounded-md border border-line bg-surface p-4 text-sm text-muted"
      :role="stateRole"
      :aria-live="stateRole === 'status' ? 'polite' : undefined"
    >
      {{ stateCopy }}
    </div>

    <template v-else-if="score">
      <dl class="mt-4">
        <div class="flex flex-wrap items-baseline justify-between gap-2 border-b border-line py-3">
          <dt class="font-bold text-ink">{{ explanationMessage("score.feasibility") }}</dt>
          <dd class="font-mono text-ink">
            {{ isZero(score.feasibility) ? "Passed" : "Failed" }}
            ({{ formatSigned(score.feasibility) }})
          </dd>
        </div>
      </dl>

      <ol class="mt-4 space-y-4" aria-label="Objectives in evaluation order">
        <li
          v-for="(level, index) in score.levels"
          :key="level.levelId"
          class="rounded-md border border-line bg-surface p-4"
        >
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <p class="text-sm font-bold text-muted">
                {{ explanationMessage("score.level", { level: index + 1 }) }}
              </p>
              <h3 class="font-bold text-ink">
                {{ levelLabels?.[level.levelId] ?? level.levelId }}
              </h3>
              <p class="text-sm text-muted">{{ directionLabel(level.direction) }}</p>
            </div>
            <output
              class="font-mono font-bold text-ink"
              :aria-label="`Level value ${formatSigned(level.value)}`"
            >
              {{ formatSigned(level.value) }}
            </output>
          </div>

          <dl v-if="Object.keys(level.categoryBreakdown).length" class="mt-3 space-y-2">
            <div
              v-for="(value, categoryId) in level.categoryBreakdown"
              :key="categoryId"
              class="flex justify-between gap-4 text-sm"
            >
              <dt class="text-muted">{{ categoryLabels?.[categoryId] ?? categoryId }}</dt>
              <dd class="font-mono text-ink">{{ formatSigned(value) }}</dd>
            </div>
          </dl>
          <p v-else class="mt-3 text-sm text-muted">No category contributions at this level.</p>
        </li>
      </ol>

      <section class="mt-5" aria-labelledby="score-contributions-heading">
        <h3 id="score-contributions-heading" class="font-bold text-ink">
          Assignment contributions
        </h3>
        <ul v-if="contributions.length" class="mt-2 space-y-2">
          <li v-for="contribution in contributions" :key="contribution.evidenceId">
            <button
              type="button"
              class="flex w-full items-center justify-between gap-4 rounded-md border border-line bg-raised px-3 py-2 text-left text-ink"
              @click="emit('selectContribution', contribution)"
            >
              <span>
                {{ levelLabels?.[contribution.levelId] ?? contribution.levelId }}
                <span v-if="contribution.categoryId" class="text-muted">
                  · {{ categoryLabels?.[contribution.categoryId] ?? contribution.categoryId }}
                </span>
              </span>
              <span class="font-mono font-bold">{{ formatSigned(contribution.value) }}</span>
            </button>
          </li>
        </ul>
        <p v-else class="mt-2 text-sm text-muted">
          No assignment-level contributions were recorded.
        </p>
      </section>
    </template>
  </Card>
</template>
