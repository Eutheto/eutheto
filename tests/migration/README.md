<!-- SPDX-License-Identifier: Apache-2.0 -->

# Migration tests

Reserved for compatibility fixtures for every released database, document, bundle, and pack version. Once formats exist, cases must prove ordered sequential upgrade, semantic round-trip and required unknown-extension preservation, unknown-newer refusal before mutation, malformed/bounded-input rejection, transactional rollback or interruption behavior, and backup/original retention where required.

Inputs and expected results must name their owning format versions and remain synthetic or sanitized. The detailed principles are in [`migrations/`](../../migrations/README.md). Phase 00 defines no schema version and contains no migration case or passing fixture.
