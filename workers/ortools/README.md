<!-- SPDX-License-Identifier: Apache-2.0 -->

# OR-Tools worker boundary

This directory reserves the out-of-process OR-Tools worker boundary defined by [ADR-004](../../docs/adr/004-ortools-worker.md). **Phase 00 does not fetch, bundle, compile, install, or run OR-Tools and does not build a worker executable.** OR-Tools 9.15 is only an unapproved candidate. No source URL, source digest, protobuf selection, CMake cache entry, linkage choice, or license inventory is approved here.

`VERSION` therefore contains the non-version sentinel `UNRESOLVED-PHASE-03`. It is not a release version and must never appear in a distributed manifest. Phase 03 must replace it with the reviewed project-worker version in the same change that adds the real worker and its executable checks.

## Source contract gate

`source-contract.schema.json` defines the review input that Phase 03 must close. An approved contract records:

- one exact OR-Tools source version, HTTPS source URL, and lowercase SHA-256;
- one exact protobuf source/hash plus mutually tested `protoc` and C++ runtime versions;
- the solver-worker wire version and SHA-256 of the authoritative `.proto`;
- the project worker identity/version; and
- the exact reviewed CMake cache entries, rather than flags guessed from another OR-Tools release.

`source-contract.example.json` is deliberately schema-valid but marked `unapproved-example`; every unresolved value is literal `UNRESOLVED`. It is documentation, not build input, a lock, evidence of availability, or permission to download anything. An approved instance is generated from reviewed locked inputs, is canonical UTF-8 JSON with lexicographically ordered object keys and one trailing LF, and is never hand-edited after approval.

Configuration without an absolute path supplied as `EUTHETO_ORTOOLS_PHASE3_CONTRACT` fails before any source or compiler is used. The example fails because it is unapproved. Even a structurally approved contract reaches a final Phase-00 stop: there is intentionally no no-op target or dummy executable. Phase 03 must replace that stop with the real source build only after its source, protobuf, callback, assumption-core, license, target, linkage, packaging, and benchmark gates pass.
