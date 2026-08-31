<!-- SPDX-License-Identifier: Apache-2.0 -->

# Contributing repository automation

This file applies to changes under `.github/`. The repository-wide [contribution guide](../CONTRIBUTING.md), [agent instructions](../AGENTS.md), active [Phase 02 contract](../docs/roadmap/02-domain-pack-and-planning-ir-contracts.md), and [security policy](../SECURITY.md) remain authoritative.

## Before changing automation

1. Identify the active phase, work-package or issue ID and the exact acceptance evidence the change serves.
2. Use the checked-in `Justfile`, Nix environment, and `xtask` entry points. A workflow must call the same repository command a contributor can run; it must not hide a second build or generation convention in YAML.
3. Preserve closed gates. A job that cannot yet run must say `unavailable` or `deferred` and must not emit passing evidence. Do not claim solver, packaged desktop E2E, signing, updater, benchmark, publication, or production-identity evidence that does not exist. Phase 01 fuzz and unbundled Linux Tauri E2E jobs report only completed executions.
4. Keep changes narrow. Do not combine a workflow policy change with unrelated dependency or generated-output drift.
5. Treat `CODEOWNERS` as the ownership map. While only one active maintainer exists, do not create an impossible self-approval gate; restore required independent ownership review when a second qualified maintainer is active.

## Workflow requirements

The required repository workflow files are `pr.yml`, `portable.yml`, `security.yml`, `benchmark.yml`, `fuzz.yml`, `release.yml`, and `dependency-update.yml`. The portable workflow owns the current unbundled Linux Tauri E2E and desktop launch smoke. Packaged desktop E2E remains deferred to the release phases.

Every workflow change must satisfy all applicable rules:

- Pin each external action to a verified full 40-character commit SHA. Put the latest compatible stable upstream tag in a nearby comment; never use a mutable tag or branch.
- Set workflow-level `permissions: contents: read` by default, then grant only the narrower job-level permission an operation requires. Do not grant write permission to validation jobs.
- Add event-appropriate concurrency groups. Cancel superseded branch and pull-request validation; do not cancel an in-progress protected signing or publication operation merely because another ref appeared.
- Protect `main` against force pushes, deletion, and non-linear history. Required independent `CODEOWNERS` approval is temporarily disabled while the repository has one active maintainer and must be restored when a second qualified maintainer is active.
- Treat pull-request code, titles, bodies, labels, paths, artifacts, caches, and dependency scripts as untrusted. A fork pull request must receive no repository or environment secret and must not gain a privileged execution path through `pull_request_target`.
- Use locked Nix for canonical Linux commands. Native Windows and macOS jobs must use the exact shared Rust, Node, and pnpm pins and the documented platform prerequisite verifier.
- Use deterministic cache keys derived from runner platform, architecture, exact lockfiles/toolchain inputs, and a deliberate cache schema version. Restore-only public caches are preferred for untrusted pull requests. Never cache credentials, signing state, `.env` files, user data, or unsanitized reports.
- Give artifacts explicit short retention, stable names, and failure behavior. Sanitize paths, environment values, logs, diagnostics, scenarios, databases, tokens, and signing metadata before upload.
- Fix locale, time zone, clock/seed/thread controls, and temporary directories where the exercised command depends on them.
- Keep build and signing jobs separate. A signing job must use a protected environment, download only the intended build artifact, verify its recorded digest before access to signing material, and export no secret to Nix, caches, logs, or ordinary artifacts.
- Never publish solely because a branch, tag, or local build exists. Publication, updater, signing identities, and production identifiers stay behind their roadmap gates.

## Dependency-update automation

Automated and manual dependency-update pull requests use committed, frozen `flake.lock`, `Cargo.lock`, `pnpm-lock.yaml`, action SHAs, worker source/hash metadata, and generated protocol evidence.

- Keep major Rust, Node, Tauri, OR-Tools, and schema migrations in separate, independently reviewable pull requests. Do not create a combined major-version rollup.
- Block new or changed JavaScript install scripts until the exact package and script are reviewed and allowlisted with a rationale.
- Flag cryptography, parsers and other untrusted-input boundaries, updater/signing, keyring/credentials, worker protocol, and Tauri permissions/capabilities for ownership review. Enforce an independent approval once a second qualified maintainer is active.
- A bot cannot create its own license, security, generated-drift, ownership, benchmark, or review exception.
- Run only gates that exist and apply. Solver benchmarks and other later-phase evidence remain deferred until the governing contract and executable command exist.

## Issue and pull-request metadata

Blank public issues are disabled. Bug and feature forms require a phase or issue ID, reproducible evidence, and confirmation that no security material is present. Security reports are never accepted through public issues; the issue chooser links to the current private-reporting policy without inventing a contact.

The pull-request template requires DCO sign-off, exact verification evidence, generated-drift and compatibility review, and explicit security, accessibility, licensing/SBOM, benchmark, automation, and release impact. Do not remove a checklist item merely because it is not applicable to one change; record `N/A — reason` instead.

## Review evidence

Review the rendered issue forms and pull-request template as contributor-facing interfaces. For workflow changes, inspect the event, permissions, concurrency, cache/artifact handling, external action pins, and secret reachability in addition to exercising the narrow repository command. Report exactly what was verified; YAML parsing alone does not prove runner behavior.
