<!-- SPDX-License-Identifier: Apache-2.0 -->

# Solver source contract and installed manifest

A native solver is accepted only when its reviewed build input and its installed evidence agree. [ADR-004](../adr/004-ortools-worker.md) requires process isolation and a matched OR-Tools/protobuf contract; [ADR-016](../adr/016-release-evidence.md) requires exact source, build, protocol, capability, linkage, target, and license evidence. These records do not make a solver trusted: every future candidate remains untrusted until independently projected and verified.

Phase 03 has approved the OR-Tools 9.15.6755 source-stage input and matched protobuf 33.1 source/toolchain in the generated [`workers/ortools/source-contract.json`](../../workers/ortools/source-contract.json). The Nix and native Windows builders now turn those authorities plus measured target-build facts into a finalized worker/runtime artifact with exact licenses, NOTICE, SPDX SBOM, and installed manifest. This build artifact is not yet an application package or available backend.

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

Phase 03 defines one installed manifest from the approved source contract plus measured build and payload evidence. The strict v1 schemas are [`workers/ortools/solver-manifest.schema.json`](../../workers/ortools/solver-manifest.schema.json), [`workers/ortools/solver-build-evidence.schema.json`](../../workers/ortools/solver-build-evidence.schema.json), and [`workers/ortools/solver-payload-evidence.schema.json`](../../workers/ortools/solver-payload-evidence.schema.json). These schemas and the Rust assembler/validator are reusable tooling; installed manifests remain target build outputs rather than checked-in files.

After a target builder has produced and postprocessed its executable and runtime closure, it invokes `cargo xtask solver finalize-artifact` with explicit authority, work, artifact, target, compiler, and source-date arguments. The finalizer derives the measured build and payload evidence from the retained CMake caches and fixed source roots; installs the exact reviewed license texts and deterministic NOTICE; creates an artifact-specific SPDX 2.3 document; assembles `<artifact-root>/solver-manifest.json`; and validates the complete nonsymlink inventory and all hashes. The lower-level `assemble-manifest` command remains available for explicit build/payload evidence inputs. `cargo xtask solver validate-manifest` independently requires the source contract, protocol schema, protocol policy, and artifact root and validates their agreement, canonical bytes, exact inventory, and every referenced file.

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

Build evidence repeats the tested backend ID, adapter version, and capability set; a mismatch is rejected rather than conflated with later descriptor registration. Both measured CMake scopes must repeat all 26 approved typed source-contract entries exactly and may add only the reviewed target-specific keys used by the Nix and native Windows builders. Fixed booleans, `Ninja`, policy mode, and the `EIGEN_MPL2_ONLY` compiler flag retain their reviewed typed/value forms. Windows additionally requires and records `CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded`; recursive PE inspection rejects any remaining non-system VC runtime import. Compiler identity is normalized to `clang`, `gcc`, or `msvc`, with a bounded ASCII version rather than a raw compiler banner. Host paths never enter evidence. A single path-valued normalized cache string must start with exactly one of `@artifact-root@`, `@build-root@`, `@source-root@`, `@target-root@`, or `@toolchain-root@`; the platform loader spellings `@loader_path` and `@rpath` are also preserved explicitly. Token paths use `/` and contain no empty, `.`, `..`, colon, second token, list separator, assignment, home-expansion, or whitespace component. Absolute paths, drive paths, backslashes, raw home/build-host paths, unreviewed keys, and unknown `@` tokens are rejected.

The installed manifest is carried beside the worker and payload. The later worker handshake returns the SHA-256 of its exact canonical bytes, and the supervisor compares that digest with the manifest selected from the application bundle before accepting health or any later capability. Neither process resolves the worker or its manifest from ambient `PATH`. Paths are bundle-relative forward-slash paths only: absolute paths, empty segments, `.`/`..`, drive prefixes, home-directory expansion, and duplicate canonical aliases are forbidden. The artifact root, manifest, and every payload path component must be a regular nonsymlink path; target builders dereference selected runtime-library links into loader-name files before finalization. Canonical containment under the artifact root is rechecked before hashing.
Every selected license source is checked against a reviewed SHA-256 before its bytes are copied or assigned an SPDX identifier. Repository-owned `LICENSE` and `NOTICE` files are pinned to LF checkout semantics; the generated artifact NOTICE preserves the project copyright attribution and records the fixed upstream source/version/hash facts.

## Canonical encoding and bounds

Both records use UTF-8 JSON without a byte-order mark, two-space indentation, lexicographically ordered object keys, no insignificant trailing spaces, and exactly one trailing LF. Duplicate object keys, non-integer numbers, non-finite numbers, unpaired Unicode surrogates, and invalid UTF-8 are rejected. Hash text is exactly 64 lowercase hexadecimal characters. Arrays whose order is not semantic are sorted by their documented stable key; capabilities are sorted by identifier and unique, while file inventories are sorted by relative path and unique.

Parsing is bounded before the record is trusted:

| Item | Maximum |
|---|---:|
| complete manifest | 65,536 bytes |
| artifact-specific SPDX SBOM | 1,048,576 bytes |
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

The current [`workers/ortools/VERSION`](../../workers/ortools/VERSION) value is the reviewed project worker version `0.1.0`. The Nix and native Windows builders now consume the approved OR-Tools/protobuf sources and repository patch, retain exact target-specific CMake/compiler evidence, build and smoke-test the worker, copy only its recursive runtime closure, and finalize the exact license/NOTICE/SBOM/manifest payload after binary postprocessing.

Phase-03 completion must still:

1. bind the installed manifest digest into the worker handshake and supervisor checks;
2. prove manifest-validated installation and smoke behavior without ambient `PATH`;
3. prove clean cross-target regeneration yields byte-identical contracts, manifests, protocol products, and release evidence; and
4. assemble and validate the unsigned sidecar before backend registration.

The source builders are available only on their documented targets. Build-level installed-manifest generation is available, while `xtask` installation/smoke, production packaging, backend registration, signing, and release readiness remain unavailable until their later gates close; no current worker artifact may be represented as bundled or release-ready.
