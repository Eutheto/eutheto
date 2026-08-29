<!-- SPDX-License-Identifier: Apache-2.0 -->

# Migrations

This directory reserves ownership for public-document, bundle, and database migration artifacts. Phase 01 defines the first schemas and migration registries; Phase 00 does not invent a schema version, SQL file, document transform, compatibility fixture, or successful migration.

## Compatibility principles

- **Sequential:** a migration has one explicit source version and one explicit destination version. Supported upgrades apply every intervening step in order; no step silently skips an intermediate contract. Document migrations are pure, deterministic, bounded, and offline. Database migrations are ordered, append-only after release, and transactional.
- **Round-trip:** after a supported input is parsed and upgraded, canonical serialization and reopening must preserve its supported meaning and all unknown extension data that the owning format promises to retain. This is a semantic preservation check, not a promise of lossy downgrade support or a requirement that bytes remain identical across versions.
- **Unknown newer:** a reader must reject a document, bundle, pack, protocol, or database version newer than it supports before mutation. It must not guess, coerce, silently downgrade, or partially commit newer data.

Future migration fixtures must cover each released step, sequential upgrade to current, semantic round-trip, unknown-newer refusal, malformed and bounded input, transactional rollback/interruption, backup retention where required, and unchanged original input on failure. Fixture and expected-result versions must be explicit.

An empty migration directory, a registry with no cases, or a zero-case test run is not migration evidence. See the [fixture contract](../docs/architecture/test-fixtures.md), [ADR-018](../docs/adr/018-public-scenario-representation.md), and the [Phase-01 migration requirements](../docs/roadmap/01-core-application-shell-and-persistence.md#autosave-startup-recovery-migrations).
