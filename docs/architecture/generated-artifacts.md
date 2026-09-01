<!-- SPDX-License-Identifier: Apache-2.0 -->

# Generated Artifacts

## Status and authority

[Phase 00](../roadmap/00-repository-and-reproducible-tooling.md#human-command-and-generation-contract) establishes generation ownership and reproducibility policy. The `Justfile` is the human command authority; `xtask` owns cross-platform generation, stable serialization, hashing, target handling, protocol verification, solver assembly metadata, fixtures, license notices/inventories, smoke SBOMs, and release manifests. [ADR-012](../adr/012-tauri-api-and-generated-dtos.md) applies this rule to desktop DTOs, [ADR-016](../adr/016-release-evidence.md) applies it to release evidence, and the contributor procedure is in [generated code and contracts](../contributors/generated-code-and-contracts.md).

This policy does not imply that a future schema, solver binding, worker, artifact-specific release SBOM, or release manifest already exists. Exact OR-Tools/protobuf inputs, source hashes, and build flags remain unapproved until the [Phase-03 gate](../roadmap/03-ortools-worker-vertical-slice.md); no generator may fill those fields with guessed or placeholder values.

## Ownership model

A generated artifact is evidence derived from authoritative inputs. It is never an independent source of truth.

| Artifact family | Authoritative input | Generator owner | Delivery phase |
|---|---|---|---|
| TypeScript desktop DTOs and applicable schemas | Reviewed Rust application contract types and schema/version declarations | `xtask generate`, exposed by the repository generation command | Phase-00 shell contract; expanded by the phase introducing each API/schema |
| Tauri ACL manifests and JSON schemas under `apps/desktop/src-tauri/gen/` | Tauri configuration, capabilities, permissions, command/plugin metadata, and the pinned Tauri dependency/CLI set | The Tauri CLI/build invoked by `just desktop-dev` or `just desktop-build`; not `xtask` | Phase 00 native desktop shell |
| Worker protocol bindings, descriptors, and protocol fixtures | Project-owned protocol source, version/compatibility declaration, and the approved matched generator/runtime/protobuf inputs | `xtask generate` and `xtask protocol verify` | Protocol shape may be reserved in Phase 00; usable peer and exact matched set are Phase 03 |
| Worker source/hash and solver manifests | Exact reviewed worker source, approved fetched solver/protobuf inputs, target/build/linkage facts, and produced artifact bytes | `xtask` solver/hash/manifest logic | Phase 03; exact packaged-bundle evidence is Phase 11 |
| Third-party notices and license inventories | Committed `Cargo.lock`, committed `pnpm-lock.yaml`, and reviewed `xtask/supply-chain-inputs.json` workspace identities/license conclusions | `cargo xtask licenses generate` writes `THIRD_PARTY_NOTICES.md` and `xtask/generated/license-inventory.json` | Phase-00 locked-workspace inventory; exact assembled-artifact notices are Phase 11 |
| SBOM inputs and SBOM products | The same committed locks and reviewed static input used by the license inventory | `cargo xtask sbom generate` writes `xtask/generated/sbom.spdx.json` | Deterministic SPDX-2.3 JSON locked-workspace smoke in Phase 00; artifact-specific release SBOMs are Phase 11 and verified in Phase 12 |
| Release/version manifests, checksum inputs, and compatibility matrices | Immutable source/locks, authoritative version declarations, build flags, target manifests, and exact artifact digests | `cargo xtask release assemble-manifest` and the protected release workflow | Explicitly unavailable until Phase 11 supplies finalized identity and artifacts; publication approval is Phase 12 |
| Generated documentation or support matrices | Current schemas, manifests, tests, and support declarations | The owning `xtask` generator | The phase that introduces the authoritative contract |

### Phase-02 domain and solver contract products

`cargo xtask generate` owns the Phase-02 contract products below. `cargo xtask generate-check` verifies their complete inventory and exact bytes.

| Authoritative input | Checked-in outputs |
|---|---|
| `schemas/domain-packs/official-test.contract.json` (source schema version 1) | `crates/eutheto-command/src/generated_official_test_pack_contract.rs`, `apps/desktop/src/api/generated-domain-pack-contracts.ts`, `schemas/generated/official-test.command-schemas.json`, `schemas/generated/official-test.portable.schema.json`, `schemas/generated/official-test.share-result.schema.json`, `xtask/generated/official-test-ai-tools.json`, `xtask/generated/official-test-ui-manifest.json`, and `docs/generated/official-test-pack-contract.md` |
| `schemas/solver-support-matrix.json` (matrix schema version 1) | `crates/eutheto-solver-api/src/generated_support_matrix.rs` and `docs/generated/solver-support-matrix.md` |

The generator parses both inputs with unknown-field rejection, validates sorted unique identities and complete metadata, and emits stable pretty JSON or deterministic source/document tables. Generated source and Markdown headers record a BLAKE3 digest of the exact authoritative input bytes. JSON products cannot carry ownership comments without changing their public schema, so this inventory records their authority and generator instead.

The command names above are the required repository contract, not a claim that a successful release artifact has been generated. Ad hoc shell scripts, IDE generators, post-processors, and manually copied tool output do not own any artifact family.

### Desktop generator boundaries

The desktop has two deliberately distinct generator owners:

- `apps/desktop/src/api/generated.ts` is project-generated TypeScript. `cargo xtask generate` owns its bytes, `cargo xtask generate-check` rejects drift, and the normal frontend formatter and linter continue to check it.
- `apps/desktop/src-tauri/gen/` contains Tauri-owned ACL manifests and schemas. The pinned Tauri CLI emits these bytes while running the native development or build command. They are not `xtask` products and are excluded from Prettier because a general formatter must not rewrite another generator's output.

Checked-in Tauri schemas and ACL manifests must match fresh output from the pinned Tauri CLI and their authoritative configuration, capability, permission, command/plugin metadata, and dependency inputs. A generated diff is reviewed and committed with the input or pinned-tool change that caused it; unexplained drift is a failure, not a formatting task. Contributors regenerate through the repository Tauri development/build command rather than hand-editing these JSON files or running general-purpose formatters over them.

### Phase-00 supply-chain products

The Phase-00 license inventory and smoke SBOM deliberately describe both Cargo and pnpm workspace packages and every dependency package recorded in their committed locks. Their bytes depend only on `Cargo.lock`, `pnpm-lock.yaml`, `xtask/supply-chain-inputs.json`, and repository-owned generator code. The reviewed static input supplies first-party workspace identity, a fixed SPDX document namespace/time, and exact component-specific license conclusions. A dependency without such a conclusion is emitted as `NOASSERTION`; generation remains factual and deterministic, while release assembly remains blocked rather than guessing a license.

Missing locks, unsupported lock versions, malformed package records, stale workspace identities, and stale reviewed conclusions are typed generator failures. `cargo xtask generate-check` compares the desktop source, notice, license inventory, and smoke SBOM with their authoritative inputs without rewriting them. `cargo xtask release verify-clean` additionally requires a clean tracked Git tree and tracked supply-chain inputs/outputs, then performs generated drift checks and worker-protocol verification. `cargo xtask release assemble-manifest` is a real gate failure until Phase 11 provides final product identity, target artifacts, digests, and protected build/sign evidence.


## Checked-in outputs

Generated sources are checked in only when a release build consumes them or onboarding materially benefits. Each checked-in output must be listed by the generation inventory with:

- authoritative input paths and applicable versions;
- generator command and pinned tool identity/version;
- expected output path and file mode;
- canonicalization rules;
- source-input hash or adjacent manifest reference; and
- owning phase or subsystem.

Where syntax permits comments, a generated file starts with the appropriate SPDX identifier and an unambiguous generated/do-not-edit header naming the authoritative input and repository generator. Formats that cannot safely contain comments must not receive invalid metadata fields; their adjacent manifest or generation inventory carries the same ownership and hash information.

A source hash is computed from the declared canonical authoritative input set using the repository hashing implementation. It is not copied from a mutable web page, inferred from a version string, or entered to satisfy a schema. When upstream source bytes are required, the recorded digest must be verified from the exact fetched archive/source. A change to the declared inputs, generator, canonicalization rules, or toolchain must regenerate and review the affected hash and products together.

## Deterministic double generation

Generation must be a pure function of the checked-in source, locked inputs, explicit target, and pinned toolchain. The deterministic-generation check performs the following in isolated clean temporary output roots:

1. generate output set A from the declared inputs;
2. discard generator process state and generate output set B from the same inputs;
3. compare complete inventories, relative paths, bytes, and executable modes; and
4. fail on any addition, deletion, byte difference, or mode difference.

Both runs use fixed clock/epoch where a format needs one, locale, time zone, seed, thread policy, line endings, path normalization, archive ordering, map/set ordering, and compression settings. Generated bytes must not contain temporary/workstation paths, wall-clock timestamps, host locale, ambient usernames, random identifiers, nondeterministic traversal order, or ambient tool versions. If a product legitimately records time, the time is an explicit authoritative input such as the release source epoch, not the current clock.

## Clean-tree drift check

After double generation passes, the drift check regenerates through the canonical repository command from an otherwise clean checkout and compares the complete generated inventory with checked-in products. It fails for:

- changed bytes or file modes;
- missing, renamed, or unexpected generated files;
- stale headers, source hashes, manifests, or inventory entries;
- checked-in output not owned by a declared generator; or
- output that changes only because the host, path, clock, locale, ordering, or ambient tool differs.

The check does not update files in place and then declare success. Its success condition is zero drift before and after the check. CI and dependency-update workflows run the same check contributors run; release preflight applies it to the exact locked source used for candidate construction.

## Never hand-edit

Never hand-edit a generated DTO, schema, protocol binding/descriptor, derived fixture, support matrix, notice, SBOM, source hash, solver manifest, checksum manifest, provenance input, or release manifest. To change output:

1. change the authoritative source or generator;
2. update the compatibility or migration contract when public meaning changes;
3. regenerate every affected product using the pinned repository command;
4. review semantic and generated diffs, including headers and hashes; and
5. run double-generation and clean-tree drift evidence.

Manual repair, post-processing, copied local output, and CI-only rewrites hide the causal change and are prohibited. Generated artifacts are reviewed and committed with their authoritative input and generator changes in one coherent change set.

## Release boundary

Phase 11 generates notices, SBOMs, manifests, checksums, and provenance from the exact assembled target bundles. Unsigned build facts and post-signing/notarization artifact facts remain distinguishable, because signing and timestamps can change bytes. [Release policy](../releases.md) requires digest verification across the protected build/sign boundary and binds final evidence to exact artifact digests. Phase 12 verifies that evidence; Phase 00 neither enables the workflow nor makes a release claim.
