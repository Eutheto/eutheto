<script setup lang="ts">
import { computed, useId } from "vue";

import type {
  AssignmentComparisonV1,
  AssignmentLockStateV1,
  AssignmentValue,
  DomainAssignment,
  DomainEntityRef,
  ExplanationCertainty,
  Int64,
  MetricValue,
  RuleEvaluation,
  RunTerminalOutcomeV1,
  SolutionComparisonV1,
  Uint64,
} from "../../api/generated";
import { Badge, Card } from "../ui";
import { explanationMessage } from "./messages";
import type { ExplanationUiState } from "./types";

const props = withDefaults(
  defineProps<{
    comparison: SolutionComparisonV1 | null;
    state: ExplanationUiState;
    entityLabels?: Readonly<Record<string, string>>;
    ruleLabels?: Readonly<Record<string, string>>;
    locale?: string | null;
  }>(),
  {
    entityLabels: () => ({}),
    ruleLabels: () => ({}),
    locale: null,
  },
);

const emit = defineEmits<{
  selectChange: [change: AssignmentComparisonV1];
}>();

const headingId = useId();
const addedHeadingId = useId();
const removedHeadingId = useId();
const changedHeadingId = useId();
const numberFormatter = computed(() => new Intl.NumberFormat(props.locale ?? undefined));
const readyComparison = computed(() =>
  props.state === "ready" && props.comparison ? props.comparison : null,
);
const added = computed(
  () =>
    readyComparison.value?.assignments.filter(
      (change): change is Extract<AssignmentComparisonV1, { readonly type: "added" }> =>
        change.type === "added",
    ) ?? [],
);
const removed = computed(
  () =>
    readyComparison.value?.assignments.filter(
      (change): change is Extract<AssignmentComparisonV1, { readonly type: "removed" }> =>
        change.type === "removed",
    ) ?? [],
);
const changed = computed(
  () =>
    readyComparison.value?.assignments.filter(
      (change): change is Extract<AssignmentComparisonV1, { readonly type: "changed" }> =>
        change.type === "changed",
    ) ?? [],
);
const hasChanges = computed(() => {
  const comparison = readyComparison.value;
  return Boolean(
    comparison &&
    (comparison.assignments.length > 0 ||
      comparison.rules.length > 0 ||
      comparison.scoreLevels.length > 0 ||
      comparison.metrics.length > 0 ||
      comparison.locks.length > 0 ||
      comparison.runs !== null),
  );
});
const countAnnouncement = computed(() => {
  const comparison = readyComparison.value;
  if (!comparison) return "";
  const assignmentCount = added.value.length + removed.value.length + changed.value.length;
  const proofCount = comparison.runs === null ? 0 : 1;
  const total =
    assignmentCount +
    comparison.rules.length +
    comparison.scoreLevels.length +
    comparison.metrics.length +
    comparison.locks.length +
    proofCount;
  const format = numberFormatter.value;
  return `${format.format(total)} verified changes: ${format.format(added.value.length)} added, ${format.format(removed.value.length)} removed, ${format.format(changed.value.length)} changed assignments; ${format.format(comparison.rules.length)} rule, ${format.format(comparison.scoreLevels.length)} score, ${format.format(comparison.metrics.length)} metric, ${format.format(comparison.locks.length)} lock, and ${format.format(proofCount)} run or proof changes.`;
});
const stateText = computed(() => {
  if (props.state === "loading") return "Loading verified changes…";
  if (props.state === "stale") return explanationMessage("error.stale");
  if (props.state === "cancelled") return explanationMessage("error.cancelled");
  if (props.state === "inconclusive") return explanationMessage("error.inconclusive");
  if (props.state === "unavailable") return explanationMessage("error.unavailable");
  if (props.state === "internalFailure") return explanationMessage("error.internal");
  return explanationMessage("change.empty");
});

function entityLabel(entity: DomainEntityRef): string {
  return (
    props.entityLabels[`${entity.kind}:${entity.id}`] ??
    props.entityLabels[entity.id] ??
    `${entity.kind} ${entity.id}`
  );
}

function formatInt64(value: Int64): string {
  try {
    return numberFormatter.value.format(BigInt(value));
  } catch {
    return value;
  }
}

function formatUint64(value: Uint64): string {
  return typeof value === "number" ? numberFormatter.value.format(value) : formatInt64(value);
}

function valueText(value: AssignmentValue): string {
  if (value.type === "boolean") return value.value ? "Yes" : "No";
  if (value.type === "integer") return formatInt64(value.value);
  if (value.type === "absent") return "Not assigned";
  return `Interval ${formatInt64(value.value.start)}–${formatInt64(value.value.end)} (duration ${formatInt64(value.value.duration)})`;
}

function assignmentValue(assignment: DomainAssignment): string {
  return valueText(assignment.value);
}

function ruleStatus(evaluation: RuleEvaluation | null): string {
  if (!evaluation) return "Not present";
  return evaluation.satisfied ? "Satisfied" : "Not satisfied";
}

function metricText(value: MetricValue | null): string {
  if (!value) return "Not present";
  if (value.type === "integer") return formatInt64(value.value);
  return `${formatInt64(value.value.numerator)} / ${formatUint64(value.value.denominator)}`;
}

function lockText(lock: AssignmentLockStateV1): string {
  return lock.state === "unlocked" ? "Unlocked" : `Locked to ${valueText(lock.value)}`;
}

function outcomeText(outcome: RunTerminalOutcomeV1): string {
  if (outcome.type === "accepted") {
    return outcome.status === "optimal" ? "Accepted optimal result" : "Accepted feasible result";
  }
  if (outcome.type === "verificationAlarm") return "Internal verification alarm";
  if (outcome.type === "interrupted") return "Interrupted";
  const labels: Record<(typeof outcome)["status"], string> = {
    infeasible: "Infeasibility proven",
    unbounded: "Unbounded model proven",
    noSolutionWithinLimit: "No verified result within the limit",
    cancelled: "Cancelled",
    invalidModel: "Invalid model",
    backendUnavailable: "Backend unavailable",
    backendFailed: "Backend failed",
  };
  return labels[outcome.status];
}

function certaintyText(certainty: ExplanationCertainty): string {
  const labels: Record<ExplanationCertainty, string> = {
    deterministic: "Deterministic",
    independentlyVerified: "Independently verified",
    backendProof: "Backend proof",
    sufficientConflict: "Sufficient conflict",
    provenMinimalConflict: "Proven minimal conflict",
    inconclusive: "Inconclusive",
    unavailable: "Unavailable",
  };
  return labels[certainty];
}

function entityHref(entity: DomainEntityRef): string {
  return `#entity-${encodeURIComponent(entity.kind)}-${encodeURIComponent(entity.id)}`;
}

function ruleHref(ruleId: string): string {
  return `#rule-${encodeURIComponent(ruleId)}`;
}
</script>

<template>
  <Card
    as="section"
    :variant="state === 'internalFailure' ? 'danger' : 'surface'"
    :aria-labelledby="headingId"
  >
    <h2 :id="headingId" class="font-display text-lg font-bold text-ink">
      {{ explanationMessage("change.heading") }}
    </h2>

    <template v-if="readyComparison && hasChanges">
      <p class="mt-2 text-sm text-muted" aria-live="polite" aria-atomic="true">
        {{ countAnnouncement }}
      </p>

      <div class="mt-4 grid gap-4 lg:grid-cols-3">
        <section :aria-labelledby="addedHeadingId">
          <h3 :id="addedHeadingId" class="font-semibold text-ink">
            {{ explanationMessage("change.added") }} ({{ added.length }})
          </h3>
          <ul v-if="added.length" class="mt-2 space-y-2">
            <li v-for="change in added" :key="`added:${change.after.id}`">
              <button
                type="button"
                class="w-full rounded-md border border-line bg-raised p-3 text-left text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                :aria-label="`Select added change for ${entityLabel(change.after.entity)}`"
                @click="emit('selectChange', change)"
              >
                <Badge variant="neutral">Added</Badge>
                <span class="mt-2 block font-semibold">{{ entityLabel(change.after.entity) }}</span>
                <span class="block text-xs text-muted">{{ change.after.id }}</span>
                <span class="mt-1 block">Value: {{ assignmentValue(change.after) }}</span>
              </button>
            </li>
          </ul>
          <p v-else class="mt-2 text-sm text-muted">No added assignments.</p>
        </section>

        <section :aria-labelledby="removedHeadingId">
          <h3 :id="removedHeadingId" class="font-semibold text-ink">
            {{ explanationMessage("change.removed") }} ({{ removed.length }})
          </h3>
          <ul v-if="removed.length" class="mt-2 space-y-2">
            <li v-for="change in removed" :key="`removed:${change.before.id}`">
              <button
                type="button"
                class="w-full rounded-md border border-line bg-raised p-3 text-left text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                :aria-label="`Select removed change for ${entityLabel(change.before.entity)}`"
                @click="emit('selectChange', change)"
              >
                <Badge variant="outline">Removed</Badge>
                <span class="mt-2 block font-semibold">{{
                  entityLabel(change.before.entity)
                }}</span>
                <span class="block text-xs text-muted">{{ change.before.id }}</span>
                <span class="mt-1 block">Previous value: {{ assignmentValue(change.before) }}</span>
              </button>
            </li>
          </ul>
          <p v-else class="mt-2 text-sm text-muted">No removed assignments.</p>
        </section>

        <section :aria-labelledby="changedHeadingId">
          <h3 :id="changedHeadingId" class="font-semibold text-ink">
            {{ explanationMessage("change.changed") }} ({{ changed.length }})
          </h3>
          <ul v-if="changed.length" class="mt-2 space-y-2">
            <li v-for="change in changed" :key="`changed:${change.after.id}`">
              <button
                type="button"
                class="w-full rounded-md border border-line bg-raised p-3 text-left text-sm text-ink focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-focus"
                :aria-label="`Select changed assignment for ${entityLabel(change.after.entity)}`"
                @click="emit('selectChange', change)"
              >
                <Badge variant="accent">Changed</Badge>
                <span class="mt-2 block font-semibold">{{ entityLabel(change.after.entity) }}</span>
                <span class="block text-xs text-muted">{{ change.after.id }}</span>
                <span class="mt-1 block">Before: {{ assignmentValue(change.before) }}</span>
                <span class="block">After: {{ assignmentValue(change.after) }}</span>
              </button>
            </li>
          </ul>
          <p v-else class="mt-2 text-sm text-muted">No changed assignments.</p>
        </section>
      </div>

      <section
        v-if="readyComparison.affectedEntities.length"
        class="mt-5"
        aria-label="Affected references"
      >
        <h3 class="font-semibold text-ink">Affected references</h3>
        <ul class="mt-2 flex flex-wrap gap-2">
          <li
            v-for="entity in readyComparison.affectedEntities"
            :key="`${entity.kind}:${entity.id}`"
          >
            <a
              :href="entityHref(entity)"
              class="font-bold text-accent-strong underline underline-offset-2"
            >
              {{ entityLabel(entity) }}
            </a>
          </li>
        </ul>
      </section>

      <section v-if="readyComparison.rules.length" class="mt-5" aria-label="Required rule changes">
        <h3 class="font-semibold text-ink">Required rule changes</h3>
        <ul class="mt-2 space-y-3">
          <li
            v-for="rule in readyComparison.rules"
            :key="rule.ruleId"
            class="rounded-md border border-line bg-raised p-3 text-sm text-muted"
          >
            <a
              :href="ruleHref(rule.ruleId)"
              class="font-bold text-accent-strong underline underline-offset-2"
            >
              {{ ruleLabels[rule.ruleId] ?? `Required rule ${rule.ruleId}` }}
            </a>
            <p>Before: {{ ruleStatus(rule.before) }}. After: {{ ruleStatus(rule.after) }}.</p>
            <ul
              v-if="rule.after?.affectedEntities.length"
              class="mt-1 flex flex-wrap gap-2"
              aria-label="Affected rule references"
            >
              <li
                v-for="entity in rule.after.affectedEntities"
                :key="`${entity.kind}:${entity.id}`"
              >
                <a
                  :href="entityHref(entity)"
                  class="font-bold text-accent-strong underline underline-offset-2"
                >
                  {{ entityLabel(entity) }}
                </a>
              </li>
            </ul>
          </li>
        </ul>
      </section>

      <section v-if="readyComparison.scoreLevels.length" class="mt-5" aria-label="Score changes">
        <h3 class="font-semibold text-ink">Score changes</h3>
        <ul class="mt-2 space-y-3">
          <li
            v-for="level in readyComparison.scoreLevels"
            :key="level.levelId"
            class="rounded-md border border-line bg-raised p-3 text-sm text-muted"
          >
            <p class="font-semibold text-ink">
              {{ level.levelId }} ({{ level.direction === "minimize" ? "Minimize" : "Maximize" }})
            </p>
            <p>
              Before: {{ formatInt64(level.before) }}. After: {{ formatInt64(level.after) }}.
              Change: {{ formatInt64(level.delta) }}.
            </p>
            <ul v-if="level.categories.length" class="mt-2 space-y-1" aria-label="Score categories">
              <li v-for="category in level.categories" :key="category.categoryId">
                {{ category.categoryId }} — Before:
                {{ category.before === null ? "Not present" : formatInt64(category.before) }}.
                After: {{ category.after === null ? "Not present" : formatInt64(category.after) }}.
                Change:
                {{ category.delta === null ? "Not available" : formatInt64(category.delta) }}.
              </li>
            </ul>
          </li>
        </ul>
      </section>

      <section v-if="readyComparison.metrics.length" class="mt-5" aria-label="Metric changes">
        <h3 class="font-semibold text-ink">Metric changes</h3>
        <ul class="mt-2 space-y-2">
          <li
            v-for="metric in readyComparison.metrics"
            :key="metric.metricId"
            class="rounded-md border border-line bg-raised p-3 text-sm text-muted"
          >
            <span class="font-semibold text-ink">{{ metric.metricId }}</span> — Before:
            {{ metricText(metric.before) }}. After: {{ metricText(metric.after) }}.
          </li>
        </ul>
      </section>

      <section v-if="readyComparison.locks.length" class="mt-5" aria-label="Lock changes">
        <h3 class="font-semibold text-ink">Lock changes</h3>
        <ul class="mt-2 space-y-2">
          <li
            v-for="lock in readyComparison.locks"
            :key="lock.assignmentId"
            class="rounded-md border border-line bg-raised p-3 text-sm text-muted"
          >
            <span class="font-semibold text-ink">{{ lock.assignmentId }}</span> — Before:
            {{ lockText(lock.before) }}. After: {{ lockText(lock.after) }}.
            {{ lock.preserved ? "The lock was preserved." : "The lock was not preserved." }}
          </li>
        </ul>
      </section>

      <section v-if="readyComparison.runs !== null" class="mt-5" aria-label="Run and proof changes">
        <h3 class="font-semibold text-ink">Run and proof changes</h3>
        <dl class="mt-2 grid gap-2 text-sm text-muted">
          <div>
            <dt class="font-semibold text-ink">Base run {{ readyComparison.runs.base.runId }}</dt>
            <dd>
              {{ outcomeText(readyComparison.runs.base.outcome) }};
              {{ certaintyText(readyComparison.runs.base.certainty) }}.
            </dd>
          </div>
          <div>
            <dt class="font-semibold text-ink">
              Candidate run {{ readyComparison.runs.candidate.runId }}
            </dt>
            <dd>
              {{ outcomeText(readyComparison.runs.candidate.outcome) }};
              {{ certaintyText(readyComparison.runs.candidate.certainty) }}.
            </dd>
          </div>
        </dl>
        <p class="mt-2 text-sm text-muted">
          The candidate comparison is {{ readyComparison.ordering }}.
        </p>
      </section>
    </template>

    <div
      v-else
      class="mt-3"
      :role="state === 'internalFailure' ? 'alert' : 'status'"
      aria-live="polite"
    >
      <p class="text-sm text-muted">{{ stateText }}</p>
    </div>
  </Card>
</template>
