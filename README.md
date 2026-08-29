# eutheto

`eutheto` is a local-first, open-source constraint-optimization platform for building plans that people can understand, verify, edit, and trust.

Users describe what **must** happen, what they **prefer**, and which trade-offs matter. The platform validates those requirements, translates them into a solver-neutral planning model, routes the model to a compatible backend, independently verifies every candidate against the original domain meaning, and presents the result in human language.

> **Project status:** planning and repository bootstrap. The implementation roadmap is complete; production code, development commands, installers, and releases do not exist yet. Work begins with [Phase 00](docs/roadmap/00-repository-and-reproducible-tooling.md).

## Product direction

The public MVP is planned to include:

- a reusable Rust core and first-class headless CLI;
- a Tauri 2 desktop application using Vue 3, TypeScript, and Vite;
- production workforce-scheduling and individual-seat event-seating domain packs;
- a solver-neutral planning IR and deterministic backend routing;
- OR-Tools CP-SAT in an isolated, versioned native worker process;
- an experimental Pumpkin backend for explicitly proven-compatible subsets;
- independent verification and authoritative score recomputation for every accepted result;
- local SQLite persistence, revisioned commands, undo/redo, repair, comparison, explanations, and import/export;
- optional provider-neutral AI that can propose only typed, validated, reviewable, reversible application actions;
- accessible, keyboard-complete primary workflows and equivalent non-canvas representations;
- cross-platform artifacts with checksums, third-party notices, solver manifests, and SPDX SBOMs.

School timetabling is the first planned post-MVP domain. Its modeling needs constrain foundation contracts without delaying the MVP.

## Principles

- **Local-first:** core planning works without an account or cloud service.
- **Human language first:** use domain concepts such as `Required`, `Preference`, and `minimum rest`, not solver jargon.
- **Deterministic core; optional AI:** AI is neither the source of truth nor the solver.
- **Verifiability over trust:** backend status never makes a candidate publishable; original domain rules are evaluated independently.
- **Human control:** scenario changes are typed, previewable, revision-checked, atomic, and undoable where semantics permit.
- **Headless architecture:** removing the desktop client must leave a useful library and CLI.
- **Accessible by design:** keyboard, screen-reader, focus, non-color, and equivalent-list requirements are implementation gates.
- **Open without premature generalization:** stable contracts precede plugins, marketplaces, arbitrary DSLs, or distributed architecture.

## Architecture

```text
Vue/Tauri desktop ─┐
Headless CLI ──────┼─> typed application API
Optional AI ──────┘            │
                                v
                     Rust application/core
                  commands · persistence · jobs
                  validation · import/export
                                │
              ┌─────────────────┴─────────────────┐
              v                                   v
       official domain packs             solver-neutral planning IR
 workforce · seating · school                        │
                                         deterministic capability router
                                              │              │
                                              v              v
                                    in-process reviewed   versioned worker
                                    backends/algorithms   OR-Tools CP-SAT
                                              │              │
                                              └──────┬───────┘
                                                     v
                                      projection and independent
                                      domain verification/scoring
```

Dependency direction is presentation → thin Tauri adapter → application services → domain packs → planning core → backend adapters and infrastructure. Domain packs never construct solver-specific objects, and solver adapters never depend on official domains.

A candidate is visible as accepted only after projection, structural validation, independent evaluation of every required domain rule, and authoritative score recomputation.

## Roadmap

The implementation plan lives in [`docs/roadmap/`](docs/roadmap/README.md). Phases are dependency gates, not calendar estimates:

| Phase | Outcome |
|---:|---|
| [00](docs/roadmap/00-repository-and-reproducible-tooling.md) | Repository, reproducible tooling, legal baseline, CI, and real desktop boundary |
| [01](docs/roadmap/01-core-application-shell-and-persistence.md) | Core types, commands, SQLite persistence, CLI/Tauri shell |
| [02](docs/roadmap/02-domain-pack-and-planning-ir-contracts.md) | Domain-pack API, planning IR, command and solver/verifier contracts |
| [03](docs/roadmap/03-ortools-worker-vertical-slice.md) | OR-Tools worker protocol and first real solver vertical slice |
| [04](docs/roadmap/04-independent-verifier-and-explanations.md) | Pack-neutral independent verification and explanation foundations |
| [05](docs/roadmap/05-workforce-core-vertical-slice.md) | Workforce domain core and first complete verified domain slice |
| [06](docs/roadmap/06-desktop-design-system-and-workforce-setup.md) | Accessible desktop design system and workforce setup experience |
| [07](docs/roadmap/07-workforce-solving-results-repair-and-export.md) | Workforce solve, results, repair, explanations, import, and export |
| [08](docs/roadmap/08-pumpkin-backend-and-router.md) | Experimental Pumpkin adapter and deterministic backend router |
| [09](docs/roadmap/09-seating-domain-and-venue-experience.md) | Seating domain, deterministic geometry, and accessible venue experience |
| [10](docs/roadmap/10-ai-assistant-mvp.md) | Optional provider-neutral AI proposal and review workflow |
| [11](docs/roadmap/11-public-mvp-packaging-and-documentation.md) | Cross-platform packaging, updater, support data, and public documentation |
| [12](docs/roadmap/12-stabilization-and-public-release-gate.md) | Stabilization, conformance, and public release gate |
| [13](docs/roadmap/13-post-mvp-roadmap.md) | School timetabling and post-MVP platform evolution |

See [`docs/roadmap/assumptions.md`](docs/roadmap/assumptions.md) for dated package/tool evidence, compatibility exceptions, and unresolved product gates.

## Current repository contents

The repository currently contains planning documentation and contributor instructions only:

```text
.
├── AGENTS.md
├── README.md
├── .gitignore
└── docs/
    └── roadmap/
```

Phase 00 will add the Cargo and pnpm workspaces, Nix flake, `Justfile`, `xtask`, CI, legal/governance files, and the minimal real Tauri/Vue application. Until those files exist, there is no supported bootstrap, build, test, or run command.

## Contributing

Read [`AGENTS.md`](AGENTS.md) before changing the repository. It defines source authority, phase discipline, architecture boundaries, generated-code rules, security and privacy constraints, and verification expectations for human and automated contributors.

Implementation work must begin with the current phase, preserve its issue IDs and exit gates, and avoid later-phase production behavior. Contributions should prefer complete vertical paths over mocks, stubs, or speculative infrastructure.

The project intends to use DCO sign-off and an Apache-2.0 license. The corresponding `DCO.md`, `LICENSE`, `CONTRIBUTING.md`, governance, security, and conduct files are Phase-00 deliverables. Until the license text is committed, this checkout should not be treated as granting an open-source license.

## Name and unresolved public identifiers

The project, Rust crate, npm package, and project-owned media-type namespaces are fixed:

- project: `eutheto`;
- Rust crates: `eutheto-*`;
- npm packages: `@eutheto/*`;
- media types: `eutheto/...`.

The final CLI executable, reverse-domain desktop IDs, portable project extension, hosting organization, governance/security contacts, and signing identities remain explicit roadmap decisions. Working examples such as `optimizer` and `.optplan` are not public commitments.
