import type { DomainEntityRef, EvidenceMessageV1, RuleEvaluation } from "../../api/generated";

export type ExplanationUiState =
  | "ready"
  | "empty"
  | "loading"
  | "stale"
  | "cancelled"
  | "inconclusive"
  | "unavailable"
  | "internalFailure";

export interface LabeledEntityRef {
  readonly entity: DomainEntityRef;
  readonly label: string;
}

export interface LabeledRuleEvaluation {
  readonly evaluation: RuleEvaluation;
  readonly label: string;
}

export interface RenderedEvidenceMessage {
  readonly evidence: EvidenceMessageV1;
  readonly text: string;
}
