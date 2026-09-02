<!-- SPDX-License-Identifier: Apache-2.0 -->

# Solver source contract and installed manifest

A native solver is accepted only when its reviewed build input and its installed evidence agree. [ADR-004](../adr/004-ortools-worker.md) requires process isolation and a matched OR-Tools/protobuf contract; [ADR-016](../adr/016-release-evidence.md) requires exact source, build, protocol, capability, linkage, target, and license evidence. These records do not make a solver trusted: every future candidate remains untrusted until independently projected and verified.

Phase 03 has approved the OR-Tools 9.15.6755 source-stage input and matched protobuf 33.1 source/toolchain in the generated [`workers/ortools/source-contract.json`](../../workers/ortools/source-contract.json). This approval permits the dependency-gated native build work; it does not provide an installed solver manifest, SBOM/license payload, packaged worker, or available backend.

## Two distinct records

### Reviewed source contract

[`workers/ortools/source-contract.schema.json`](../../workers/ortools/source-contract.schema.json) is the machine schema for the Phase-03 build input. An approved record must bind all of the following before configuration can reach a real target:

- schema version and a non-placeholder Phase-03 approval record;
- exact OR-Tools version, HTTPS source URL, lowercase SHA-256, and repository patch path/SHA-256;
- exact protobuf source version/URL/SHA-256, `protoc` version, and C++ runtime version tested as one contract;
- solver-worker wire version and SHA-256 of the authoritative `solver-worker.proto` bytes;
- official worker identity and project worker version; and
- exact reviewed CMake cache entries for that pinned source release.

[`workers/ortools/source-contract.example.json`](../../workers/ortools/source-contract.example.json) demonstrates shape only. Its status is `unapproved-example` and unresolved values are the literal string `UNRESOLVED`. The example must remain rejected by the CMake approval gate. It is not copied into a lockfile, given synthetic zero hashes, used as a network source, or transformed into release evidence.

An approved contract is produced from reviewed locked inputs, validated against the schema before CMake, and committed through the repository generation workflow. Its approval record names a full immutable repository revision; that tree binds the candidate probe, repository-owned `9.15-candidate-fixes.patch`, and the patch's exact bzip2 revision. Generation rehashes the patch and protocol schema, while CMake requires the supplied contract to be byte-identical to the generated approval and independently verifies both repository file digests. Later build logic must still apply the exact patch. Changing OR-Tools, protobuf source/runtime/generator, protocol bytes, worker version, patch, or any CMake cache entry creates a new review input and invalidates dependent generated evidence. A digest is measured from fetched bytes; maintainers never infer or type a plausible digest.

### Installed solver manifest

Phase 03 defines one installed manifest from the approved source contract plus measured build and payload evidence. The strict v1 schemas are [`workers/ortools/solver-manifest.schema.json`](../../workers/ortools/solver-manifest.schema.json), [`workers/ortools/solver-build-evidence.schema.json`](../../workers/ortools/solver-build-evidence.schema.json), and [`workers/ortools/solver-payload-evidence.schema.json`](../../workers/ortools/solver-payload-evidence.schema.json). These schemas and the Rust assembler/validator are reusable tooling; no checked-in installed manifest or official artifact exists yet.

After a target builder has produced its final executable/runtime filenames and the later license/SBOM work has installed its final payload, it invokes `cargo xtask solver assemble-manifest` with explicit `--source-contract`, `--protocol-schema`, `--protocol-policy`, `--build-evidence`, `--payload-evidence`, and `--artifact-root` paths. The only output is `<artifact-root>/solver-manifest.json`; it is created without replacement and uses mode `0644` on Unix. `cargo xtask solver validate-manifest` requires the same `--source-contract`, `--protocol-schema`, and `--protocol-policy` authority paths plus `--artifact-root`; it always validates the nonsymlink installed path `<artifact-root>/solver-manifest.json`, their agreement, canonical manifest bytes, and every referenced artifact digest. Both commands parse their explicit paths before any repository-root lookup, so an installed build-platform `xtask` is independent of a source checkout. Both print the manifest SHA-256 externally; the manifest never contains its own digest.

The manifest contains these semantic groups:

| Group | Required evidence |
|---|---|
| manifest | schema version 1 and generation-contract version 1 |
| approval | Phase-03 source-contract approval record and SHA-256 of the exact canonical source-contract bytes |
| backend source | `ortools` kind, exact upstream version, HTTPS source URL, and source archive SHA-256 |
| build | supported target triple, exactly derived architecture, compiler identity/version, `static-ortools` linkage, and separate lexically keyed normalized OR-Tools and worker CMake maps |
| capabilities | the exact sorted tested set: `cp-sat`, `deterministic-time`, `intermediate-solutions`, `objective-bounds`, `progress`, `solution-projection`, `solution-stats` |
| protobuf | source version/URL/SHA-256, exact `protoc` and linked C++ runtime versions, and approved `cp_model.proto`/`sat_parameters.proto` checksums |
| protocol | policy major/minor, source-contract wire version, and SHA-256 of the authoritative protocol schema bytes |
| worker | source-contract identity/version, `ortools-cp-sat` backend ID, adapter `0.1.0`, `bundled-worker` distribution, `beta` stability, and executable relative path/SHA-256 |
| runtime libraries | sorted relative path/SHA-256 records; empty for a completely static runtime inventory |
| licenses | sorted relative path/SHA-256 records whose SPDX identifier is one of the dependency licenses already reviewed in the Phase-03 gate: `Apache-2.0`, `BSD-3-Clause`, `MIT`, `Zlib`, or `bzip2-1.0.6` |
| SBOM | relative path and SHA-256 of the artifact-specific SBOM |

Build evidence repeats the tested backend ID, adapter version, and capability set; a mismatch is rejected rather than conflated with later descriptor registration. Both measured CMake scopes must repeat all 26 approved typed source-contract entries exactly and may add only the reviewed target-specific keys used by the Nix and native Windows builders. Fixed booleans, `Ninja`, policy mode, and the `EIGEN_MPL2_ONLY` compiler flag retain their reviewed typed/value forms. Compiler identity is normalized to `clang`, `gcc`, or `msvc`, with a bounded ASCII version rather than a raw compiler banner. Host paths never enter evidence. A single path-valued normalized cache string must start with exactly one of `@artifact-root@`, `@build-root@`, `@source-root@`, `@target-root@`, or `@toolchain-root@`; the platform loader spellings `@loader_path` and `@rpath` are also preserved explicitly. Token paths use `/` and contain no empty, `.`, `..`, colon, second token, list separator, assignment, home-expansion, or whitespace component. Absolute paths, drive paths, backslashes, raw home/build-host paths, unreviewed keys, and unknown `@` tokens are rejected.

The installed manifest is carried beside the worker and payload. The later worker handshake returns the SHA-256 of its exact canonical bytes, and the supervisor compares that digest with the manifest selected from the application bundle before accepting health or any later capability. Neither process resolves the worker or its manifest from ambient `PATH`. Paths are bundle-relative forward-slash paths only: absolute paths, empty segments, `.`/`..`, drive prefixes, home-directory expansion, and duplicate canonical aliases are forbidden. A symlinked artifact path is accepted only when every symlink target is relative and its canonical target remains under the artifact root; absolute or escaping symlinks are rejected. Canonical containment under the artifact root, regular-file status, and each recorded digest are checked. Assembly and validation require a quiescent staging/install tree owned by the invoking build or application process; pathname validation is not an authorization boundary for a concurrently hostile writer. Secrets, usernames, build-host paths, wall-clock timestamps, network credentials, and signing material are forbidden.

## Canonical encoding and bounds

Both records use UTF-8 JSON without a byte-order mark, two-space indentation, lexicographically ordered object keys, no insignificant trailing spaces, and exactly one trailing LF. Duplicate object keys, non-integer numbers, non-finite numbers, unpaired Unicode surrogates, and invalid UTF-8 are rejected. Hash text is exactly 64 lowercase hexadecimal characters. Arrays whose order is not semantic are sorted by their documented stable key; capabilities are sorted by identifier and unique, while file inventories are sorted by relative path and unique.

Parsing is bounded before the record is trusted:

| Item | Maximum |
|---|---:|
| complete manifest | 65,536 bytes |
| JSON nesting depth | 8 |
| members in one object | 128 |
| entries in one array | 256 |
| one string value | 2,048 UTF-8 bytes |
| capabilities | 64 |
| CMake cache entries | 128 |
| runtime libraries | 128 |
| license records | 256 |

Checked arithmetic is required for lengths and counts. Unknown `schema_version` values fail as unsupported before build or launch; missing required fields, unknown fields, malformed hashes, unsorted/duplicate entries, or limit violations fail validation. Readers do not silently downgrade, fill defaults, or preserve an unknown record as if it were approved. A schema revision reserves removed field meanings and includes compatibility fixtures for every accepted version.

## Phase-03 closure

The current [`workers/ortools/VERSION`](../../workers/ortools/VERSION) value is the reviewed project worker version `0.1.0`. The generated approved source contract closes only the source-stage decision. Phase-03 completion must still:

1. consume the approved OR-Tools/protobuf sources, repository patch, and exact cross-target CMake cache entries in the Nix and Windows native builders;
2. preserve target-specific compiler, generator, verified fetched-source override, `EIGEN_MPL2_ONLY`, package-path, and install-path facts as measured build inputs rather than forcing paths into the cross-target source contract;
3. build the real worker, run executable/manifest/protocol checks where the target permits, and install only the required runtime and license files;
4. generate and validate the canonical installed manifest and artifact-specific SBOM/license payload, then bind the manifest digest into handshake tests; and
5. prove clean regeneration yields byte-identical contracts, manifests, protocol products, and release evidence.

The source builders are available only on their documented targets. Installed-manifest generation, `xtask` installation/smoke, production packaging, backend registration, and release readiness remain unavailable until the remaining steps close; no current worker artifact may be represented as bundled or release-ready.
