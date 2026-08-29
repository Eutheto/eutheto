# SPDX-License-Identifier: Apache-2.0

set dotenv-load := false

# Show the canonical repository commands.
default:
    @just --list

# Fetch and install every locked workspace dependency explicitly.
bootstrap: install

# Install locked Rust and JavaScript dependencies without updating lockfiles.
install:
    cargo fetch --locked
    pnpm install --frozen-lockfile --ignore-scripts

# Regenerate checked-in deterministic artifacts.
generate:
    cargo xtask generate

# Reject generated artifact drift.
generate-check:
    cargo xtask generate-check

# Verify the versioned worker-protocol assets.
protocol-check:
    cargo xtask protocol verify

# Validate repository fixtures and discovery boundaries.
fixtures-check:
    cargo xtask fixtures validate

# Format Rust, JavaScript/TypeScript, and Nix sources.
fmt:
    cargo fmt --all
    pnpm run format
    for file in flake.nix nix/*.nix; do nixfmt "$file"; done

# Check Rust, JavaScript/TypeScript, and Nix formatting without changing files.
fmt-check:
    cargo fmt --all -- --check
    pnpm run format:check
    for file in flake.nix nix/*.nix; do nixfmt --check "$file"; done

# Run the strict Rust and JavaScript/TypeScript linters.
lint:
    cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
    pnpm run lint

# Type-check the JavaScript/TypeScript workspace.
typecheck:
    pnpm run typecheck

# Run the Rust workspace tests with default-shell tools.
test-rust:
    cargo test --workspace --all-targets --locked

# Run the Rust workspace tests through nextest in the full shell.
test-rust-nextest:
    cargo nextest run --workspace --locked

# Run Rust documentation tests.
test-doc:
    cargo test --workspace --doc --locked

# Run the UI unit and component tests.
test-ui:
    pnpm run test

# Run all Phase-00 Rust, documentation, and UI tests.
test: test-rust test-doc test-ui

# Produce the Rust workspace coverage report.
coverage:
    cargo llvm-cov nextest --workspace --locked

# Evaluate the gated Nix OR-Tools worker derivation contract.
worker-build-nix:
    nix build --impure --expr 'let flake = builtins.getFlake (toString ./.); in flake.legacyPackages.${builtins.currentSystem}.ortools-worker-contract'

# Build the native OR-Tools worker once its Phase-03 pin is approved.
worker-build-native:
    cargo xtask solver build-native

# Install the Nix-built OR-Tools worker once its Phase-03 pin is approved.
worker-install-from-nix:
    cargo xtask solver install-from-nix

# Smoke-test the OR-Tools worker once its Phase-03 pin is approved.
worker-smoke:
    cargo xtask solver smoke

# Run the real, deliberately non-final Phase-00 CLI status command.
cli:
    cargo run --locked --package eutheto-cli -- status

# Run the Vue/Vite development server without the native desktop shell.
ui-dev:
    pnpm --filter @eutheto/desktop run dev

# Run the Tauri development shell.
desktop-dev:
    pnpm --filter @eutheto/desktop run tauri dev

# Build the Phase-00 CLI.
cli-build:
    cargo build --locked --package eutheto-cli

# Build the Vue/Vite frontend.
ui-build:
    pnpm --filter @eutheto/desktop run build

# Build the native Phase-00 desktop shell without release bundles.
desktop-build:
    pnpm --filter @eutheto/desktop run tauri build --no-bundle

# Run solver benchmarks after the Phase-03 benchmark corpus is approved.
bench:
    {{ error("benchmarks are unavailable until Phase 03 approves the OR-Tools pin and a real primitive benchmark corpus") }}

# Run packaged desktop E2E tests after the Phase-11 packaging gate is complete.
e2e:
    {{ error("desktop E2E is unavailable until Phase 11 defines supported packaged targets and WebDriver prerequisites") }}

# Enforce Rust advisory policy, including the reviewed deny.toml exceptions.
rust-advisories:
    python3 -c 'from datetime import date, datetime, timezone; import sys; expiry = date.fromisoformat("2026-11-30"); today = datetime.now(timezone.utc).date(); sys.exit(f"RustSec exception review expired on {expiry}; update or remove each exception") if today > expiry else None'
    cargo deny --locked check advisories licenses bans sources
    cargo audit --deny warnings \
        --ignore RUSTSEC-2024-0370 \
        --ignore RUSTSEC-2024-0411 \
        --ignore RUSTSEC-2024-0412 \
        --ignore RUSTSEC-2024-0413 \
        --ignore RUSTSEC-2024-0414 \
        --ignore RUSTSEC-2024-0415 \
        --ignore RUSTSEC-2024-0416 \
        --ignore RUSTSEC-2024-0417 \
        --ignore RUSTSEC-2024-0418 \
        --ignore RUSTSEC-2024-0419 \
        --ignore RUSTSEC-2024-0420 \
        --ignore RUSTSEC-2024-0429 \
        --ignore RUSTSEC-2025-0075 \
        --ignore RUSTSEC-2025-0080 \
        --ignore RUSTSEC-2025-0081 \
        --ignore RUSTSEC-2025-0098 \
        --ignore RUSTSEC-2025-0100

# Generate and validate the Phase-00 third-party license inventory.
licenses:
    cargo xtask licenses generate

# Generate the Phase-00 software bill of materials.
sbom:
    cargo xtask sbom generate

# Evaluate all lightweight pinned Nix checks.
nix-check:
    nix flake check

# Exercise accepted and rejected repository DCO samples.
dco-self-test:
    python3 scripts/check_dco.py --self-test

# Run the complete non-deferred Phase-00 repository suite.
check: generate-check protocol-check fixtures-check dco-self-test fmt-check lint typecheck test

# Run real clean-tree checks, then stop at the unresolved Phase-11 release gate.
release-preflight: check
    cargo xtask release verify-clean
    cargo xtask release assemble-manifest
