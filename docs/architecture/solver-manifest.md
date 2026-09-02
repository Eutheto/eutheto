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

Phase 03 generates one installed manifest from the approved source contract plus measured build outputs. It is carried beside the worker and license material; the worker handshake returns the SHA-256 of its exact canonical bytes. The supervisor compares that digest with the manifest selected from the application bundle before accepting health or any later capability. Neither process resolves the worker or its manifest from ambient `PATH`.

The generated installed record must contain these semantic groups; Phase 03 supplies the concrete schema together with the real builder rather than publishing a fabricated Phase-00 instance:

| Group | Required evidence |
|---|---|
| manifest | manifest schema version and generation contract version |
| worker | identity, project worker version, executable filename, executable SHA-256 |
| backend source | backend kind, exact upstream version, source URL, source SHA-256 |
| protobuf contract | source version/URL/SHA-256, exact generator and linked runtime versions |
| protocol | wire version and authoritative schema SHA-256 |
| build | target triple, linkage mode, compiler identity/version, and sorted effective CMake cache entries |
| capabilities | sorted unique capability identifiers actually exercised by worker tests |
| runtime | sorted runtime library filenames and SHA-256 values when dynamically linked |
| licenses | sorted relative license paths, SHA-256 values, and reviewed SPDX expressions |
| approval | immutable reference to the source-contract approval evidence |

The installed manifest does not contain its own digest; that would be self-referential. Its SHA-256 is computed over the complete canonical file and is carried by the handshake and release evidence. Paths are bundle-relative forward-slash paths only: absolute paths, empty segments, `.`/`..`, drive prefixes, home-directory expansion, and symlink escapes are forbidden. Secrets, usernames, build-host paths, wall-clock timestamps, network credentials, and signing material are forbidden.

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

Until those steps occur, solver `xtask` operations and production packaging remain unavailable, and no worker artifact may be represented as bundled or release-ready.
