<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-013: AI Command and Trust Boundary

- **Status:** Approved; provider implementation is deferred
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

Provider output is untrusted and may be incorrect, malicious, stale, or influenced by scenario content. Optional AI assistance cannot become a second application authority, solver, verifier, or unrestricted automation channel. The application must remain fully useful when AI is disabled.

## Binding decision

> AI reads bounded context and proposes typed commands; it cannot bypass validation, write arbitrary files, run arbitrary commands, or generate solver source.

Provider responses are parsed into bounded, versioned proposal data. A proposal is previewed and, if accepted, passes through the same typed, validated, revision-checked command service as any other mutation.

## Consequences

- AI receives only explicit bounded context and typed query/tool capabilities.
- AI has no direct persistence, credential-store, filesystem, shell, code-execution, solver, or verifier authority.
- Provider content and tool arguments are validated as untrusted input; prompt text cannot grant new capabilities.
- Deterministic evidence remains visible and authoritative; AI may paraphrase but not replace it.
- Phase 00 establishes only the boundary and does not provide an AI provider or fake responses.

## Rejected alternatives

- Arbitrary file, shell, code execution, or solver-source generation is rejected.
- Direct AI writes to SQLite or bypass of preview/validation is rejected.
- AI-selected solver routing or AI as verifier/explanation authority is rejected.
- Making AI required for core product use is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
