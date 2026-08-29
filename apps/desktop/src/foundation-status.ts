import type { FoundationStatus } from "./api/generated";

export type FoundationStatusView =
  | { readonly state: "loading" }
  | { readonly state: "ready"; readonly status: FoundationStatus }
  | { readonly state: "error" };
