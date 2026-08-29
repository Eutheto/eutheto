<!-- SPDX-License-Identifier: Apache-2.0 -->

# @eutheto/desktop — Phase-00 development shell

This is a **non-public development shell**, not a released desktop application.
It exists to prove the Vue/Vite/Tauri integration, generated API layer, capability
boundaries, and identity-gate discipline described in [Phase 00](../../docs/roadmap/00-repository-and-reproducible-tooling.md)
and [ADR-012](../../docs/adr/012-tauri-api-and-generated-dtos.md).

## What this shell contains

A single Tauri command (`app_get_foundation_status`) that returns a coarse
`FoundationStatus` value indicating whether the Phase-00 repository foundation
is wired through. The command is registered through Tauri's application manifest,
capability-scoped to the `main` window, and granted only the
`allow-foundation-status-read` permission.

The shell does **not** contain domain planning, solver, scenario, database, or
AI features. Those arrive in later phases.

## Status API flow

```text
┌──────────────┐    invoke()     ┌──────────────────────┐
│ App.vue      │ ──────────────▶ │ api/generated.ts     │
│ (Vue/Vite)   │                 │ getFoundationStatus() │
└──────────────┘                 └──────────┬───────────┘
                                            │ Tauri IPC
                                            ▼
                                 ┌──────────────────────┐
                                 │ src-tauri/src/lib.rs  │
                                 │ app_get_foundation_   │
                                 │   status()            │
                                 └──────────┬───────────┘
                                            │ delegates
                                            ▼
                                 ┌──────────────────────┐
                                 │ eutheto_core          │
                                 │ foundation_status()   │
                                 └──────────────────────┘
```

The webview never reads Rust state directly. The Rust command delegates to
`eutheto_core::foundation_status()`, which is the single authoritative source.
A unit test in `src-tauri/src/lib.rs` verifies the command returns the same
value as the core function.

## Formatting and linting

Prettier owns whitespace and layout, including the placement of content and
attributes in single-line Vue elements. ESLint owns correctness and continues
to apply the recommended Vue rules and strict, type-aware TypeScript rules to
all source files, including the generated API. The lint command keeps
`--max-warnings 0`; only `vue/singleline-html-element-content-newline` and
`vue/max-attributes-per-line` are disabled because they conflict with
Prettier's output. Do not manually reshape templates solely to satisfy those
two rules, because Prettier will restore its canonical layout.

## Generated API layer

`src/api/generated.ts` is produced by `cargo xtask generate` and checked in.
It contains:

- A `FoundationStatus` TypeScript interface matching the Rust `FoundationStatus`
  struct from `eutheto-types`.
- A `getFoundationStatus()` function that calls
  `invoke<FoundationStatus>("app_get_foundation_status")`.

This file must **never** be hand-edited. To change the API:

1. Change the authoritative Rust type in `eutheto-types` or the command
   signature in `src-tauri/src/lib.rs`.
2. Regenerate with the repository generation command.
3. Review the generated diff and run `generate-check` for drift.

See [generated code discipline](../../docs/contributors/generated-code-and-contracts.md)
and [generated artifacts](../../docs/architecture/generated-artifacts.md).

## Only-API-layer invoke rule

[ADR-012](../../docs/adr/012-tauri-api-and-generated-dtos.md) requires that
**only files under `src/api/`** import `@tauri-apps/api` (invoke, event
subscription). Vue components must not import invoke or event helpers directly;
they consume the typed functions exported by the generated API layer.

This separation ensures:

- Commands are tested and typed in one place.
- The invoke/event surface is auditable.
- Components remain presentation-focused with no IPC coupling.

## Accessibility states

The `FoundationStatusPanel` component uses ARIA roles to communicate the
three shell states:

| State     | ARIA role       | Live region          | Content                                        |
| --------- | --------------- | -------------------- | ---------------------------------------------- |
| `loading` | `role="status"` | `aria-live="polite"` | "Checking the local application foundation…"   |
| `ready`   | `role="status"` | `aria-live="polite"` | Capability name, schema version, boundary note |
| `error`   | `role="alert"`  | (implicit)           | Error message, "Try again" button              |

The heading uses `aria-labelledby` referencing `foundation-heading`. The
retry button is keyboard-accessible and fires a `retry` event handled by
`App.vue`.

## Commands

| Command                     | Registration                                    | Permission                              | Scope              |
| --------------------------- | ----------------------------------------------- | --------------------------------------- | ------------------ |
| `app_get_foundation_status` | `lib.rs` invoke handler via `generate_handler!` | `allow-foundation-status-read` (custom) | `main` window only |

New commands must be:

1. Registered in `src-tauri/src/lib.rs` through the invoke handler.
2. Declared in a Tauri permission file under `src-tauri/permissions/`.
3. Granted to the minimum required window through a capability in
   `src-tauri/capabilities/`.
4. Exposed to the webview only through a generated function in `src/api/`.

## Capability and CSP boundary

**Capabilities** are defined in `src-tauri/capabilities/main.json` and grant
the `main` window only the `allow-foundation-status-read` permission. No shell,
filesystem, or broad permissions are enabled.

**Content Security Policy** (from `tauri.conf.json`):

```text
default-src 'self';
connect-src ipc: http://ipc.localhost;
img-src 'self' data:;
script-src 'self';
style-src 'self' 'unsafe-inline'
```

The CSP blocks remote navigation, remote script loading, and `eval`-style
execution. `unsafe-inline` is required only for Vite-injected style tags
during development and will be tightened for production.

## Non-public development values

The following values are explicitly **not production identities**. They exist
only for local development and will be resolved by the identity gates in
[Phase 11](../../docs/roadmap/README.md) with approved ADRs before any
public release:

| Value                                  | Location                                            | Status                                                                |
| -------------------------------------- | --------------------------------------------------- | --------------------------------------------------------------------- |
| `dev.eutheto.phase00.desktop`          | `tauri.conf.json` identifier, `Cargo.toml` metadata | Development-only reverse-domain ID; **not** the production identifier |
| `eutheto Phase 00 development shell`   | `tauri.conf.json` productName, window title         | Provisional product name; **not** a release brand                     |
| `icon.svg` and generated desktop icons | `src-tauri/icons/`                                  | Phase-00 placeholder mark; **not** a final logo or trademark          |
| Bundle version `0.1.0`                 | `tauri.conf.json`                                   | Development version; **not** a released version                       |

See [identity gates](../../docs/architecture/identity-gates.md) for the full
list of unresolved product, packaging, and signing gates.

## Running locally

```bash
pnpm install
pnpm tauri dev
```

Requires the pinned Rust toolchain (see `rust-toolchain.toml`) and platform
prerequisites documented in [development setup](../../docs/contributors/development.md).
