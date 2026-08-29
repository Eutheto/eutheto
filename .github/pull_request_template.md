<!-- SPDX-License-Identifier: Apache-2.0 -->

## Summary

<!-- Describe the smallest complete behavior changed and why. Do not claim behavior that was not exercised. -->

## Phase and issue

- Active phase / work-package ID:
- Issue or acceptance criterion:
- Prerequisites satisfied:
- Closed gates and explicit non-goals preserved:

## Evidence

<!-- List every command or scenario actually run and its result. Distinguish not run, unavailable, deferred, and failing from passing. A build does not prove packaging or product acceptance. -->

| Command or exercised scenario | Result | What it proves |
|---|---|---|
|  |  |  |

## Required review checklist

Complete each item or write `N/A — <reason>` beside it. Do not check an item that automation or review has not established.

### Contribution and scope

- [ ] Every commit includes my own DCO sign-off (`Signed-off-by`) and preserves the authorship/sign-off of other contributors.
- [ ] The phase, work-package or issue ID, acceptance criteria, prerequisites, and non-goals are identified above.
- [ ] The change is the smallest complete implementation; it adds no placeholder, mock authority, fake success, speculative compatibility shim, or deferred production feature.
- [ ] All affected callers, tests, authoritative documentation, and generated products change together; obsolete paths are removed when no published compatibility contract requires them.

### Tests and generated drift

- [ ] Focused tests or an exercised runtime scenario cover the observable change, relevant boundaries, and realistic errors.
- [ ] I reported only checks actually run and recorded unavailable or deferred gates as unavailable or deferred, not passing.
- [ ] Generated files were not hand-edited; authoritative inputs were changed and the repository generation command completed with no unexpected drift.
- [ ] The final generated-file drift check is clean, or no checked-in generated artifact is affected.

### Schemas, protocols, and migrations

- [ ] Versioning and compatibility policy are preserved; existing fields/tags are not reused with changed meaning.
- [ ] Applicable compatibility, round-trip, unknown-newer-version, malformed/bounded-input, golden-fixture, and migration tests pass.
- [ ] Protocol/source hashes, descriptors, DTOs, schemas, fixtures, manifests, and documentation are updated together where applicable.

### Security and privacy

- [ ] Untrusted request/input and response/output boundaries are both validated, bounded, and covered by evidence where applicable.
- [ ] No secret, credential, signing material, private data, captured scenario, local database, or unsanitized diagnostic enters source, logs, artifacts, caches, Nix derivations, frontend state, ordinary IPC, or pull-request jobs.
- [ ] Tauri commands, capabilities, permissions, CSP, filesystem/network access, CI permissions, and release environments remain least privilege.
- [ ] Security-sensitive paths have maintainer review; this public pull request does not disclose an unremediated vulnerability.

### Accessibility and user impact

- [ ] Changed primary UI behavior is keyboard-complete, has correct focus and screen-reader semantics, does not rely on color alone, and provides an accessible equivalent for visualizations.
- [ ] Applicable normal, empty, loading, stale, error, cancellation, and offline-capable states were exercised.

### Dependencies, licenses, and SBOM

- [ ] New or changed code, dependencies, assets, fonts, datasets, examples, and generated material have recorded source/license data and satisfy Apache-2.0/SPDX/notice policy.
- [ ] Lockfiles are frozen and intentional; dependency updates isolate each major Rust, Node, Tauri, OR-Tools, or schema migration family in an independently reviewable pull request.
- [ ] New or changed package install scripts are explicitly identified and reviewed rather than implicitly trusted.
- [ ] Applicable license policy, notices, dependency inventory, advisory checks, and SBOM generation complete without unexplained drift.

### Performance, automation, and release

- [ ] Performance-sensitive changes include the applicable benchmark result and threshold comparison; otherwise the benchmark impact is stated as `N/A` with a reason.
- [ ] Workflow actions are pinned to full 40-character commits with the compatible stable tag recorded; permissions are minimal, caches deterministic, and uploaded artifacts sanitized.
- [ ] Pull-request jobs receive no secrets and do not execute untrusted arbitrary scripts with elevated permissions.
- [ ] Release build and signing remain separate with artifact digest verification; protected signing, updater, publication, and production-identity gates remain closed unless their governing phase has explicitly closed them.

## Reviewer focus

<!-- Name the highest-risk invariants, files, or decisions that deserve particular attention. -->
