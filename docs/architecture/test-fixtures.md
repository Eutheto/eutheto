<!-- SPDX-License-Identifier: Apache-2.0 -->

# Test fixture contracts

This document defines repository boundaries for future fixtures; it does not claim that a scenario format, migration, solver, benchmark, or passing fixture exists in Phase 00. Fixture content arrives only with the phase that implements and exercises the corresponding behavior.

## Reproducible execution context

Every fixture runner must make the inputs that can affect an observable result explicit rather than inheriting them from the host:

- a fixed clock instant;
- an explicit IANA time zone and daylight-saving gap/overlap policy where time is involved;
- an explicit locale, including formatting and collation assumptions where relevant;
- a recorded pseudorandom seed;
- a fixed thread count or an explicitly documented concurrency schedule; and
- an isolated per-run temporary directory with deterministic setup and cleanup expectations.

A fixture that relies on the host clock, host time zone, host locale, available core count, shared temporary state, or ambient environment is not reproducible evidence. Secret values and machine-specific absolute paths must not enter fixtures, snapshots, logs, or expected outputs.

## Data provenance and privacy

Committed scenarios and corpora must be synthetic or demonstrably sanitized and licensed for redistribution. They must not contain captured customer, employee, student, patient, attendee, credential, support-bundle, or production data. Sanitization review must consider free text, identifiers, timestamps, filenames, metadata, and small-group combinations that could re-identify a person; merely changing display names is insufficient.

Each nontrivial fixture must identify its purpose, owning format/version, provenance or generation method, and applicable license. Synthetic generation must itself be deterministic when its output is committed or compared.

## Inputs and expected results

Fixture inputs and expected results are separate artifacts. Expected results must declare the fixture/input version and every contract version needed to interpret them. Depending on the test, that can include the scenario or database schema, domain-pack contract, planning IR, worker protocol, backend/build manifest, and expected-result format. A field or tag is never reused with a different meaning.

Correctness expectations describe observable semantics and invariants, not incidental serialization, collection order, timings, or backend-native artifacts unless those details are the contract under test. Unknown newer versions must fail safely. Unknown extension data must be preserved where the owning format requires forward-compatible round trips.

Changing a fixture or expected result requires review of the semantic difference. Regeneration is not evidence by itself, and a bulk snapshot acceptance must not hide behavior changes.

## Performance baselines

A performance baseline is versioned with its corpus, runner, toolchain, target, backend/build identity, deterministic context, warm-up and sample method, and measured metric. Baselines and regression thresholds require maintainer review when created or changed. Machine-sensitive measurements must state the eligible runner class; results from incomparable hosts must not overwrite an approved baseline.

Performance expectations never substitute for correctness verification. Phase 00 provides only the directory boundary: it contains no fabricated measurements, thresholds, solver claims, or benchmark results.

## Evidence rule

An empty directory, README, manifest without cases, ignored test, placeholder, zero-case parameterization, or runner that discovers no fixtures is not passing evidence. A fixture suite counts only when the implementing phase adds real, reviewed cases, verifies that the cases were discovered and executed, and observes the required behavior. Future fixture directories in this repository intentionally document ownership without satisfying later phase gates.

See [ADR-017](../adr/017-time-and-integer-units.md) for explicit time semantics, [ADR-018](../adr/018-public-scenario-representation.md) for versioned public formats, and the [Phase-00 roadmap](../roadmap/00-repository-and-reproducible-tooling.md) for the current boundary.
