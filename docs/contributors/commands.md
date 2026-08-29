<!-- SPDX-License-Identifier: Apache-2.0 -->

# Contributor commands

The root `Justfile` is the human command authority. Run `just --list` to inspect the commands in the checkout, and use those recipes rather than reproducing their Cargo, pnpm, Nix, or `xtask` internals in local scripts or CI.

Nix is the canonical Linux environment and supplies the language and native tools on macOS. Native Windows development uses the pinned prerequisites documented for Windows. Entering `nix develop` or activating direnv is intentionally side-effect-free: it does not install packages, fetch dependencies, generate files, run migrations, or build the solver worker. Those operations begin only when an explicit recipe is run.

## First checkout

From the repository root:

```console
nix develop
just bootstrap
just check
```

`just bootstrap` delegates to `just install`. Installation runs `cargo fetch --locked` and `pnpm install --frozen-lockfile --ignore-scripts`; it may populate project-local and tool caches, but it cannot update either lockfile or execute dependency lifecycle scripts. The committed `supportedArchitectures` matrix materializes optional packages for every supported OS/CPU so the security workflow can inspect their manifests. Run `just install` directly when dependencies alone need refreshing.

On a native platform route, first satisfy that platform's documented prerequisites, then run the same `just bootstrap` and applicable portable recipes. Nix-only recipes require Nix.

## Command reference

### Dependencies and generated artifacts

| Recipe | Behavior |
| --- | --- |
| `just bootstrap` | Explicit first-checkout entry point; installs the locked Rust and pnpm dependency graphs. |
| `just install` | Fetches Cargo dependencies with `--locked` and installs pnpm dependencies with `--frozen-lockfile`. |
| `just generate` | Delegates deterministic checked-in artifact generation to `cargo xtask generate`. |
| `just generate-check` | Rejects drift between authoritative inputs and checked-in generated artifacts. |
| `just protocol-check` | Verifies the versioned worker-protocol assets through `xtask`. |
| `just fixtures-check` | Validates repository fixture discovery and filesystem boundaries through `xtask`. |

Never hand-edit a generated product. Change its authoritative input, run `just generate`, inspect the result, and run `just generate-check`.

### Static checks

| Recipe | Behavior |
| --- | --- |
| `just fmt` | Formats the Rust workspace, pnpm workspace, `flake.nix`, and the Nix modules. |
| `just fmt-check` | Checks the same sources without modifying them. The pinned `nixfmt` is invoked with its supported `--check` file form. |
| `just lint` | Runs strict workspace Clippy and the pnpm lint command. |
| `just typecheck` | Runs the pnpm workspace TypeScript checks. |
| `just nix-check` | Runs the lightweight flake checks with `nix flake check`. It is separate from the full suite. |
| `just check` | Runs non-mutating generated-output drift checks through `generate-check`, including the existing checked-in license/SBOM outputs, plus protocol and fixture verification, formatting, linting, type checking, and all Phase-00 tests. It does not regenerate supply-chain files. Deferred worker, packaged E2E, and release assembly gates are deliberately not hidden in this passing Phase-00 suite. |

`nix flake check` validates the lightweight reproducibility contract; it does not replace `just check`.

### Tests, coverage, and benchmarks

| Recipe | Behavior |
| --- | --- |
| `just test-rust` | Runs all Rust workspace targets with locked `cargo test`; it is the default-shell Rust test recipe. |
| `just test-rust-nextest` | Optionally runs the Rust workspace through `cargo nextest` when using the full shell. |
| `just test-doc` | Runs Rust documentation tests. |
| `just test-ui` | Runs pnpm UI unit and component tests. |
| `just test` | Runs the Rust, documentation, and UI test recipes. |
| `just coverage` | Runs the Rust workspace through `cargo llvm-cov nextest` with the full-shell tools. Coverage is diagnostic and does not replace behavioral tests. |
| `just bench` | Fails until Phase 03 approves the OR-Tools pin and a real primitive benchmark corpus; Phase 00 does not report an empty Cargo benchmark run as evidence. |
| `just e2e` | Fails with the Phase-11 packaged-desktop/WebDriver gate until supported targets and prerequisites exist. It does not pretend that a Vite browser test proves a packaged Tauri application. |

### Applications and builds

| Recipe | Behavior |
| --- | --- |
| `just cli` | Runs `optimizer status`, the real but explicitly non-final Phase-00 CLI behavior. |
| `just ui-dev` | Starts the Vue/Vite development server without the native shell. |
| `just desktop-dev` | Starts the Tauri development shell. Native WebView prerequisites apply. |
| `just cli-build` | Builds the locked `eutheto-cli` package. |
| `just ui-build` | Type-checks and builds the Vue/Vite frontend through its package script. |
| `just desktop-build` | Builds the native Phase-00 Tauri shell with release bundling disabled. This does not claim installer, updater, signing, or public application-identity readiness. |

The CLI name and desktop identifier used by Phase 00 are development surfaces, not a decision about public naming or registration.

### Solver worker

| Recipe | Behavior in Phase 00 |
| --- | --- |
| `just worker-build-nix` | Evaluates the Nix worker derivation contract and fails at its unresolved OR-Tools source/hash gate. |
| `just worker-build-native` | Delegates to `xtask` and fails at the same Phase-03 source, protobuf, hash, and license gate. |
| `just worker-install-from-nix` | Delegates installation to `xtask`; it cannot install an unapproved or dummy worker. |
| `just worker-smoke` | Delegates executable smoke testing to `xtask`; it cannot report success before a real worker exists. |

These failures are intentional. Phase 03 must approve one exact OR-Tools source, hash, matched protobuf contract, build flags, and license inventory before any worker recipe may succeed.

### Supply chain and release

| Recipe | Behavior |
| --- | --- |
| `just licenses` | Explicitly regenerates and validates the Phase-00 Cargo and pnpm license inventory and notices through `xtask`. |
| `just sbom` | Explicitly regenerates the Phase-00 Cargo and pnpm SBOM outputs through `xtask`. |
| `just release-preflight` | Runs the complete Phase-00 check, verifies that tracked release inputs are clean, then fails at release-manifest assembly until the Phase-11 public identity, artifact, and signing gates are resolved. |

Successful `just licenses` or `just sbom` runs prove that the current Phase-00 workspace inventory was regenerated; they do not approve a future solver bundle or release artifact. The non-mutating `just check` path validates these existing outputs through `generate-check` instead. `release-preflight` is intentionally not a successful release command in Phase 00.

## Reporting verification

Record the exact recipes and relevant platform used in a pull request. State only what each run proves: unit tests do not prove packaging, a successful build does not prove runtime behavior, and the Phase-00 shell does not prove deferred solver or release capability.
