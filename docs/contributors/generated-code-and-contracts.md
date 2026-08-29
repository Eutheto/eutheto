<!-- SPDX-License-Identifier: Apache-2.0 -->

# Generated Code, Schema, and Protocol Discipline

This guidance applies to human and automated contributors. It implements the [Phase-00 generation contract](../roadmap/00-repository-and-reproducible-tooling.md#human-command-and-generation-contract), [ADR-012](../adr/012-tauri-api-and-generated-dtos.md), and [ADR-018](../adr/018-public-scenario-representation.md).

## Authority order

When representations disagree, use this order:

1. approved security and data-integrity ADRs;
2. current scenario, bundle, and worker-protocol schemas plus conformance tests;
3. the [roadmap](../roadmap/README.md) and active phase;
4. generated documentation and generated sources; and
5. code comments.

A generated file is evidence of its input and generator, never a second source of truth.

## Generated-file ownership

`xtask` owns cross-platform generation, stable serialization, hashing, target handling, protocol verification, solver assembly, fixtures, license notices, SBOM inputs, and release manifests. The `Justfile` is the human command authority. Use the checked-in repository command rather than an ad hoc shell or local IDE generator. The Phase-00 command contract names `just generate` and `just generate-check`; if a command is not yet present during bootstrap, do not claim or simulate a successful generation.

Generated products may be checked in only when release builds consume them or onboarding materially benefits. Every checked-in generated file must make its authority discoverable through its path, header, adjacent manifest, or generation inventory. Where the file format permits comments, include an SPDX identifier and a clear generated/do-not-edit marker. Machine formats that cannot safely contain comments rely on the generation inventory rather than invalid extra fields.

Never:

- hand-edit a generated DTO, schema, protocol output, support matrix, notice, SBOM, fixture derivative, source hash, or release manifest;
- post-process generated output with an unowned script;
- accept output that embeds workstation paths, wall-clock time, locale, nondeterministic map order, random seeds, or ambient tool versions;
- generate during Nix shell entry, application startup, a migration, or another implicit side effect; or
- commit build output, local caches, test reports, databases, credentials, captured scenarios, or unsanitized support bundles.

Change the authoritative source or generator, regenerate, and review both semantic and generated diffs. Running generation twice with fixed inputs must produce identical bytes. A clean-tree generation check must report no drift.

## Public format discipline

The following are public compatibility surfaces once introduced: scenario documents, portable bundles, database schemas, application commands/events/DTOs, CLI machine output, domain-pack documents, planning/projection records persisted or exchanged by contract, and the solver-worker protocol.

Every such surface requires:

- an explicit format/protocol version and a documented compatibility policy;
- stable field and enum/tag meanings—never reuse a removed field or tag with a new meaning;
- safe refusal of unknown newer outer versions;
- preservation of unknown extension data when its format contract requires forward compatibility;
- centralized byte, item-count, nesting, string, frame, and decompression limits;
- deterministic canonical ordering and checked arithmetic for hashes or signatures;
- typed errors that distinguish malformed, unsupported, incompatible, and resource-limit failures; and
- fixtures for every supported released version and malformed boundary.

The final public project extension is unresolved. Examples and file associations must continue to use an explicitly working/internal label until its roadmap gate closes; no contributor may publish a placeholder as the final extension.

## Schema changes

For a new or changed schema:

1. identify the authoritative Rust/type/schema input and every generated consumer;
2. classify the change as compatible, migration-requiring, or breaking under the existing policy;
3. add or update migration logic before changing writers;
4. preserve unknown data where required and refuse unknown newer versions safely;
5. update round-trip, canonicalization, limits, malformed-input, unknown-version, and every-version migration fixtures;
6. regenerate all checked-in products from locked tools; and
7. run the narrow compatibility and generation-drift commands that exist for that surface.

Do not use an alias, dual writer, deprecated field, or silent fallback as a convenient cutover. Migrate every caller and remove obsolete internal paths in the same change unless a published compatibility contract specifically requires continued reading. Writers emit only the current version; readers support exactly the documented window.

## Worker protocol changes

The worker is an untrusted process boundary, not an internal function call. Protocol source, generator/runtime, upstream OR-Tools protobufs, worker build, adapter, and golden frames are one reviewed contract.

A protocol change must:

- update the authoritative protocol and version/compatibility declaration;
- retain unique field numbers and enum values permanently, reserving removed identifiers;
- define both request and response limits, lifecycle, cancellation, terminal state, and unknown-message handling;
- regenerate all language outputs using the pinned matched toolchain;
- update source/protobuf checksums and worker/adapter manifest expectations;
- cover old/new handshake behavior, truncation, oversized and malformed frames, unknown tags, duplicate/out-of-order lifecycle messages, early exit, timeout, and cancellation;
- prove malformed or mismatched worker data cannot mutate scenario state or crash the desktop process; and
- keep human names, notes, credentials, AI content, and unrelated filesystem paths out of worker payloads and diagnostics.

Do not independently upgrade `protoc`, protobuf runtime, OR-Tools protos, or one generated language binding. The exact matched set closes in Phase 03; Phase 00 must not guess it or ship a dummy protocol peer.

## Review checklist

A contract change is ready only when the authoritative input, generator, checked-in outputs, every caller, compatibility policy, migrations, malformed/limit fixtures, docs, hashes/manifests, and focused verification agree. Report only commands actually run. Formatting or compilation alone does not prove compatibility, and a source diff alone does not prove deterministic generation.
