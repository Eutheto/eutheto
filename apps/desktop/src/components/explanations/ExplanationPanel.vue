<script setup lang="ts">
import { computed } from "vue";
import type {
  DomainEntityRef,
  EvidenceMessageV1,
  ExplanationCertainty,
  ExplanationResultV1,
} from "../../api/generated";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogTitle,
  DialogTrigger,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "../ui";
import { explanationMessage, renderEvidenceTemplate } from "./messages";
import type { ExplanationUiState } from "./types";

interface Props {
  readonly open: boolean;
  readonly result: ExplanationResultV1 | null;
  readonly state: ExplanationUiState;
  readonly messageTexts: Readonly<Record<string, string>>;
}

const props = defineProps<Props>();
const emit = defineEmits<{
  "update:open": [open: boolean];
  retry: [];
  editRelated: [];
  cancel: [];
}>();
defineSlots<{
  trigger?(): unknown;
}>();

const stateCopy = computed(() => {
  switch (props.state) {
    case "ready":
      return props.result ? null : explanationMessage("explanation.empty");
    case "empty":
      return explanationMessage("explanation.empty");
    case "loading":
      return explanationMessage("error.loading");
    case "stale":
      return explanationMessage("error.stale");
    case "cancelled":
      return explanationMessage("error.cancelled");
    case "inconclusive":
      return explanationMessage("error.inconclusive");
    case "unavailable":
      return explanationMessage("error.unavailable");
    case "internalFailure":
      return explanationMessage("error.internal");
  }
});

const stateHeading = computed(() => {
  switch (props.state) {
    case "ready":
    case "empty":
      return "No explanation available";
    case "loading":
      return "Preparing verified evidence";
    case "stale":
      return "Explanation is out of date";
    case "cancelled":
      return "Explanation cancelled";
    case "inconclusive":
      return "Explanation is inconclusive";
    case "unavailable":
      return "Explanation unavailable";
    case "internalFailure":
      return explanationMessage("error.heading");
  }
});

const stateRole = computed(() =>
  props.state === "stale" || props.state === "unavailable" || props.state === "internalFailure"
    ? "alert"
    : "status",
);

function messageText(message: EvidenceMessageV1): string {
  const template =
    props.messageTexts[message.messageKey] ?? "Verified explanation evidence is available.";
  return renderEvidenceTemplate(template, message.parameters);
}

function certaintyLabel(certainty: ExplanationCertainty): string {
  switch (certainty) {
    case "deterministic":
      return "Deterministic evidence";
    case "independentlyVerified":
      return "Independently verified";
    case "backendProof":
      return "Backend proof";
    case "sufficientConflict":
      return "Sufficient conflict";
    case "provenMinimalConflict":
      return "Proven minimal conflict";
    case "inconclusive":
      return "Inconclusive";
    case "unavailable":
      return "Unavailable";
  }
}

function certaintyDescription(certainty: ExplanationCertainty): string {
  switch (certainty) {
    case "deterministic":
      return "These facts were derived deterministically. They are evidence, not a solver proof.";
    case "independentlyVerified":
      return "An independent verifier accepted the evidence shown here.";
    case "backendProof":
      return "The solver supplied proof evidence for this outcome; deterministic evidence remains listed separately.";
    case "sufficientConflict":
      return "This set is enough to prove a conflict. Other conflicts may also exist, and minimality was not proved.";
    case "provenMinimalConflict":
      return "Minimality was proved for the stated conflict grouping.";
    case "inconclusive":
      return "The available evidence does not establish a verified conclusion.";
    case "unavailable":
      return "No proof or verified certainty evidence is available.";
  }
}

function entityHref(entity: DomainEntityRef): string {
  return `#entity-${encodeURIComponent(entity.kind)}-${encodeURIComponent(entity.id)}`;
}

function ruleHref(ruleId: string): string {
  return `#rule-${encodeURIComponent(ruleId)}`;
}
</script>

<template>
  <Dialog :open="open" @update:open="emit('update:open', $event)">
    <DialogTrigger v-if="$slots.trigger" as-child>
      <slot name="trigger" />
    </DialogTrigger>
    <DialogContent class="max-w-3xl">
      <div class="flex items-start justify-between gap-4">
        <div>
          <DialogTitle>{{ explanationMessage("explanation.heading") }}</DialogTitle>
          <DialogDescription>
            Verified evidence and proof status are shown separately.
          </DialogDescription>
        </div>
        <DialogClose as-child>
          <button
            type="button"
            class="rounded-md border border-line bg-surface px-3 py-2 font-bold text-ink"
          >
            {{ explanationMessage("action.close") }}
          </button>
        </DialogClose>
      </div>

      <div
        v-if="stateCopy"
        class="rounded-md border border-line bg-surface p-4"
        :role="stateRole"
        :aria-live="stateRole === 'status' ? 'polite' : undefined"
        aria-atomic="true"
      >
        <h2 class="font-bold text-ink">{{ stateHeading }}</h2>
        <p class="mt-1 text-sm text-muted">{{ stateCopy }}</p>
        <p v-if="state === 'loading'" class="mt-1 text-sm text-muted">
          You can cancel this request instead of waiting.
        </p>
        <div class="mt-4 flex flex-wrap gap-3">
          <button
            v-if="state === 'loading'"
            type="button"
            class="rounded-md border border-line-strong bg-raised px-3 py-2 font-bold text-ink"
            @click="emit('cancel')"
          >
            {{ explanationMessage("action.cancel") }}
          </button>
          <button
            v-if="state !== 'loading' && state !== 'empty'"
            type="button"
            class="rounded-md border border-line-strong bg-raised px-3 py-2 font-bold text-ink"
            @click="emit('retry')"
          >
            {{ explanationMessage("action.retry") }}
          </button>
          <button
            v-if="state !== 'loading'"
            type="button"
            class="rounded-md border border-line bg-surface px-3 py-2 font-bold text-ink"
            @click="emit('editRelated')"
          >
            {{ explanationMessage("action.edit") }}
          </button>
        </div>
      </div>

      <Tabs v-else-if="result" default-value="evidence">
        <TabsList aria-label="Explanation sections">
          <TabsTrigger value="evidence">
            {{ explanationMessage("explanation.messages") }}
          </TabsTrigger>
          <TabsTrigger value="proof">{{ explanationMessage("explanation.proof") }}</TabsTrigger>
        </TabsList>

        <TabsContent value="evidence" force-mount>
          <section aria-labelledby="deterministic-evidence-heading">
            <h2
              id="deterministic-evidence-heading"
              class="font-display text-lg font-semibold text-ink"
            >
              Deterministic evidence
            </h2>
            <ol v-if="result.rendered.messages.length" class="mt-3 space-y-4">
              <li
                v-for="(message, index) in result.rendered.messages"
                :key="`${message.messageKey}-${index}`"
                class="rounded-md border border-line bg-surface p-4"
              >
                <p class="text-ink">{{ messageText(message) }}</p>

                <div v-if="message.entities.length || message.rules.length" class="mt-3">
                  <h3 class="text-sm font-bold text-muted">Related items</h3>
                  <ul class="mt-1 flex flex-wrap gap-2">
                    <li v-for="entity in message.entities" :key="`${entity.kind}:${entity.id}`">
                      <a
                        :href="entityHref(entity)"
                        class="font-bold text-accent-strong underline underline-offset-2"
                      >
                        {{ entity.kind }} {{ entity.id }}
                      </a>
                    </li>
                    <li v-for="ruleId in message.rules" :key="ruleId">
                      <a
                        :href="ruleHref(ruleId)"
                        class="font-bold text-accent-strong underline underline-offset-2"
                      >
                        Rule {{ ruleId }}
                      </a>
                    </li>
                  </ul>
                </div>

                <details v-if="message.evidence.length" class="mt-3">
                  <summary class="cursor-pointer text-sm font-bold text-ink">
                    Evidence references
                  </summary>
                  <ul class="mt-1 list-disc pl-5 text-sm text-muted">
                    <li v-for="evidenceId in message.evidence" :key="evidenceId">
                      {{ evidenceId }}
                    </li>
                  </ul>
                </details>
              </li>
            </ol>
            <p v-else class="mt-3 text-sm text-muted">
              {{ explanationMessage("explanation.empty") }}
            </p>
          </section>
        </TabsContent>

        <TabsContent value="proof" force-mount>
          <section aria-labelledby="proof-certainty-heading">
            <h2 id="proof-certainty-heading" class="font-display text-lg font-semibold text-ink">
              {{ explanationMessage("explanation.proof") }}
            </h2>
            <p class="mt-3 font-bold text-ink">
              Certainty: {{ certaintyLabel(result.evidence.certainty) }}
            </p>
            <p class="mt-1 text-sm text-muted">
              {{ certaintyDescription(result.evidence.certainty) }}
            </p>
            <dl class="mt-4 space-y-2 text-sm">
              <div>
                <dt class="font-bold text-muted">Evidence checksum</dt>
                <dd class="break-all font-mono text-ink">{{ result.evidence.checksum }}</dd>
              </div>
              <div>
                <dt class="font-bold text-muted">Rendered evidence checksum</dt>
                <dd class="break-all font-mono text-ink">{{ result.rendered.evidenceChecksum }}</dd>
              </div>
            </dl>
          </section>
        </TabsContent>
      </Tabs>

      <div v-if="!stateCopy" class="flex justify-end">
        <button
          type="button"
          class="rounded-md border border-line bg-surface px-3 py-2 font-bold text-ink"
          @click="emit('editRelated')"
        >
          {{ explanationMessage("action.edit") }}
        </button>
      </div>
    </DialogContent>
  </Dialog>
</template>
