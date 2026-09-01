<!-- SPDX-License-Identifier: Apache-2.0 -->

# Solver worker protocol

[`protocol/solver-worker.proto`](../../protocol/solver-worker.proto) is the sole authoritative wire schema. [`protocol/version.json`](../../protocol/version.json) is the sole authoritative compatibility and resource policy. The protobuf package is `eutheto.worker.v1`; the only checked-in negotiated version is exactly major `1`, minor `1`, and the parent advertises only that exact minor. A newer worker may speak this negotiated 1.1 dialect; bounded unknown wire fields are ignored at runtime. Any schema addition must be deliberately added with its complete reviewed descriptor signature and policy/version treatment, so later removal or retyping remains mechanically visible. The parent does not claim support for any future minor until its dialect and emission gates are implemented and checked in.

The worker is one fresh bundled process per solve. Stdout contains protocol frames only; bounded diagnostics use stderr. No C++ ABI crosses into Rust, and neither a worker status nor a projected assignment is trusted as a domain result. Every candidate remains evidence that the parent must project and independently verify.

## Transport and envelopes

Each frame is exactly:

1. a four-octet unsigned big-endian protobuf payload length; then
2. exactly that many octets of binary protobuf.

There is no compression, resynchronization, or trailing data within a frame. Receivers apply the current state-specific cap before allocation or protobuf decoding and reject zero-length, truncated, malformed, over-cap, over-nested, or out-of-order frames.

The protocol deliberately has separate directional envelopes:

- `ParentFrame`: `handshake_request = 1` or `solve_request = 2`; tags 3–15 reserved.
- `WorkerFrame`: `handshake_response = 1`, `started = 2`, `progress = 3`, `incumbent = 4`, `finished = 5`, or `error = 6`; tags 7–15 reserved.

Every solve frame carries the non-empty opaque `request_id` copied from `SolveRequest`. Handshake messages do not borrow a solve ID. `WorkerError` is a solve-terminal frame and therefore also carries the request ID; handshake rejection uses the typed `HandshakeResponse.error` outcome.

## Session state flow

The only legal flow is:

```text
START
  -> ParentFrame.handshake_request
  -> WorkerFrame.handshake_response.success
  -> ParentFrame.solve_request
  -> WorkerFrame.started
  -> zero or more WorkerFrame.progress / WorkerFrame.incumbent
  -> exactly one WorkerFrame.finished / WorkerFrame.error
  -> no further stdout bytes; clean stdout EOF
  -> process exit code 0
```

`HandshakeResponse.error` terminates before any solve. Duplicate handshake or solve requests, solve before handshake, a mismatched/stale request ID, event before `Started`, a second `Started`, event after a terminal frame, duplicate terminal frames, missing terminal frame, or incompatible exit/frame evidence is a protocol violation. The parent closes stdin after its one solve request and kills the process tree on cancellation, deadline plus grace expiry, excessive output/events, malformed protocol, or cleanup failure.

Handshake success echoes the exact accepted protocol major/minor requested by the parent. Before launch, the parent validates the installed manifest and the worker executable named by it, computes the manifest's 32-byte SHA-256, and sends that digest as `expected_manifest_sha256`. The worker echoes the exact digest in `HandshakeSuccess.manifest_sha256` only as untrusted correlation evidence. No trust in the manifest, executable, or worker derives from the echo: the manifest can hash the worker executable without circular embedding because the digest is supplied by the parent rather than embedded in the executable. Success also reports worker identity/version, backend ID, OR-Tools version, adapter version, and capabilities. The canonical request requires only `CP_SAT` and `SOLUTION_PROJECTION`; its manifest digest is 32 bytes of `0x11`, exactly matching the canonical success echo. The canonical success advertises the exact currently supported set: `CP_SAT`, `INTERMEDIATE_SOLUTIONS`, `PROGRESS`, `SOLUTION_PROJECTION`, `OBJECTIVE_BOUNDS`, `SOLUTION_STATS`, and `DETERMINISTIC_TIME`. The parent compares all expected values and required capabilities before sending a solve. Handshake failure is a typed code with bounded text and optional supported major/minor. `SUFFICIENT_ASSUMPTIONS` is allocated but deliberately absent from the advertised set while its native semantic gate remains unresolved.

## Solve and event contract

`SolveRequest` contains the request ID, raw serialized `CpModelProto` bytes, project-owned `SolveParameters`, numeric projection requests, `ResourceLimits`, and a 32-byte model fingerprint. The fingerprint is SHA-256 over the exact serialized `cp_model_proto` bytes sent on the wire, not a parsed or semantically reserialized model. Rust recomputes it before sending to validate its own request construction; the native worker must independently recompute and compare it before solving. Values echoed in `Started` and terminal frames remain untrusted and serve only as correlation and stale-frame rejection evidence. An echo never proves which model bytes were solved and never establishes candidate validity; Phase-04 independent verification remains authoritative. `SolveParameters` is an allowlist only: random seed, stop-after-first-feasible, intermediate-solution emission, diagnostic progress logging, and deterministic-test profile. Removed tags 1 and 2 are reserved: wall time and worker count exist only as `ResourceLimits.wall_time_millis` and `ResourceLimits.worker_threads`, which are the sole values used to construct the corresponding native `SatParameters`. Raw `SatParameters` is never accepted.

All generated Rust protobuf `bytes` fields use `prost::bytes::Bytes`. Decoding an owned `Bytes` frame therefore slices the large `CpModelProto`, fingerprints, and hashes without copying them into new vectors; bounded transport acquisition remains the single owned frame buffer.
Native C++ validation consumes `protocol/generated/cpp/protocol-policy.h`, the sole generated projection of `version.json`; worker source must not duplicate protocol versions, caps, or the applied-parameter hash domain separator.

Projection IDs are project-local unsigned numbers paired with CP-SAT variable indices. The worker never receives domain names or user content. `Incumbent` and the optional final candidate contain only requested projected values.

`Finished` preserves:

- the exact integer CP-SAT status;
- normalized worker status and a distinct termination reason, including `UNKNOWN` when CP-SAT returned unknown without a known limit cause;
- an optional final projected candidate and objective/bound vectors;
- optional wall, user, and deterministic time;
- optional bounded conflict, branch, binary-propagation, and integer-propagation counts;
- bounded sufficient-assumption literals when the capability is safely enabled;
- a 32-byte applied-parameters SHA-256 defined below and the submitted model fingerprint.

Unavailable evidence stays absent rather than being synthesized. `FEASIBLE` is never promoted to `OPTIMAL`, `UNKNOWN` is not mislabeled as an internal failure, and process exit evidence never turns an incomplete or unverified candidate into success.

### Applied-parameters hash

`applied_parameters_sha256` is SHA-256 over exactly this 56-byte preimage:

1. the 36 ASCII bytes `eutheto.applied-solve-parameters.v1\0`;
2. final `wall_time_millis` as unsigned 64-bit big-endian;
3. final `worker_threads` as unsigned 32-bit big-endian;
4. normalized `random_seed` as signed 32-bit big-endian two's-complement; and
5. four bytes, each exactly `0` or `1`, in this order: `stop_after_first_feasible`, `emit_intermediate_solutions`, `log_search_progress`, `deterministic_test_profile`.

Absent input defaults are `random_seed = 1` (the pinned OR-Tools default) and `false` for each optional Boolean. `memory_bytes` is excluded because it is enforced by the parent process and is not worker-applied CP-SAT behavior. A deterministic-test profile is valid only when the final worker-thread count and normalized seed are both `1`. Rust independently reconstructs and hashes this preimage and rejects a reported mismatch. A matching worker-reported hash remains untrusted reproducibility/correlation evidence: it does not prove the native code actually applied those settings. Parent deadline/thread enforcement and observed behavior remain separate evidence.

## Central policy

All ceilings are centralized in `protocol/version.json`. String counts are UTF-8 bytes; repeated-field limits count elements.

| Policy | Ceiling |
|---|---:|
| Handshake frame | 1 MiB |
| Solve-request frame | 256 MiB |
| Worker event frame | 16 MiB |
| Captured stderr | 4 MiB |
| Total session bytes | 512 MiB |
| Frames per session | 4,099 |
| Worker events per session | 4,096 |
| Worker events per second | 64 |
| Protobuf nesting depth | 8 |
| Worker threads | 10,000 |
| Any repeated field | 100,000 elements |
| Projection requests / projected values / sufficient assumptions | 100,000 elements each |
| Any string | 4,096 bytes |
| Request ID | 64 bytes |
| Core, worker, OR-Tools, and adapter version strings | 64 bytes each |
| Error message | 512 bytes |

The policy also maps each envelope route to its frame class and each security-relevant fully qualified field to its byte or count ceiling. Runtime code consumes those descriptor-keyed entries rather than duplicating field-name limits.

## Terminal frames and process exits

The worker exit code is corroborating evidence:

| Code | Meaning |
|---:|---|
| 0 | Terminal result or typed error frame written, followed by clean stdout EOF |
| 64 | Invalid invocation or protocol input |
| 65 | Invalid or unsupported model payload |
| 70 | Internal worker error |
| 71 | OR-Tools initialization or version error |
| 75 | Resource limit or temporary execution failure |
| 78 | Worker configuration or version mismatch |

A successful protocol terminal path is indivisible: the worker writes exactly one valid terminal solve frame (`Finished` or `WorkerError`; likewise a typed handshake rejection before solve), writes no further stdout bytes, closes stdout with clean EOF, and exits with code `0`. The parent accepts terminal evidence only after observing that EOF and zero exit in order. A nonzero exit is reserved for a failure where a terminal protocol frame could not be written consistently; it cannot complete a solve, authenticate a candidate, or turn partial evidence into a quarantined result. Missing EOF, bytes after the terminal frame, a nonzero exit after a terminal frame, or any other contradictory evidence is backend failure and all partial candidate evidence is discarded.

## Stable allocations

The complete field and enum allocation is mechanically checked from the generated descriptor. The principal message tags are:

| Declaration | Stable field tags |
|---|---|
| `HandshakeRequest` | major 1, minor 2, core version 3, expected backend 4, required capabilities 5, expected manifest hash 16; 6–15 and 17–31 reserved |
| `HandshakeResponse` | success 1, typed error 2; 3–15 reserved |
| `HandshakeSuccess` | major 1, minor 2, identity 3, worker version 4, backend 5, OR-Tools 6, adapter 7, manifest hash 8, capabilities 9; 10–31 reserved |
| `HandshakeError` | code 1, message 2, optional supported major 3/minor 4; 5–15 reserved |
| `SolveRequest` | request ID 1, model bytes 2, parameters 3, projections 4, resource limits 5, fingerprint 6; 7–31 reserved |
| `SolveParameters` | tags 1–2 reserved; seed 3, first-feasible 4, intermediate 5, logging 6, deterministic profile 7; 8–31 reserved |
| `ProjectionRequest` | projection ID 1, CP-SAT variable index 2; 3–15 reserved |
| `ResourceLimits` | wall milliseconds 1, optional memory bytes 2, worker threads 3; 4–15 reserved |
| `Started` | request ID 1, fingerprint 2; 3–15 reserved |
| `Progress` | request ID 1, kind 2, objective 3, bounds 4, optional wall 5/deterministic 6; 7–15 reserved |
| `Incumbent` | request ID 1, candidate 2, objective 3, bounds 4, optional wall 5/deterministic 6; 7–15 reserved |
| `ProjectedCandidate` / `ProjectedValue` | values 1; projection ID 1/value 2; remaining tags through 15 reserved |
| `Finished` | request ID 1, raw status 2, normalized status 3, termination 4, candidate 5, objectives 6, bounds 7, wall/user/deterministic time 8/9/10, conflicts 11, branches 12, binary/integer propagations 13/14, assumptions 15, parameter hash 16, model fingerprint 17; 18–31 reserved |
| `WorkerError` | request ID 1, code 2, message 3, retryable 4; 5–15 reserved |

Every enum reserves zero as `UNSPECIFIED`. Allocated nonzero values and reserved ranges are part of v1 and cannot be reused or renumbered.

## Generated artifacts and fixtures

`cargo xtask generate` requires exact `libprotoc 33.1`, runs protocol generation twice in isolated temporary roots, and compares the complete bytes before writing:

- Rust prost binding, configured with `.bytes(["."])` for owned zero-copy byte fields: `crates/eutheto-protocol/src/generated/eutheto.worker.v1.rs`;
- C++ binding: `protocol/generated/cpp/solver-worker.pb.h` and `.cc`;
- binary `FileDescriptorSet`: `protocol/generated/eutheto.worker.v1.descriptor.pb`;
- paired fixture files under `protocol/golden/`.

Fixtures include one successful handshake → solve → started → progress → incumbent → finished sequence. `worker-error` is a separate alternative solve-terminal fixture and never occurs in that successful session. The solve fixture's exact `CpModelProto` bytes are hex `120412020000`, representing variable 0 fixed to domain `[0,0]`; their SHA-256/model fingerprint is `94176f9b2ce2f9af8432215d877f1503132385a4b10ffd3f22d9851db218ac27`. Each pair has one typed Rust frame value as its source: the generator encodes that value into the complete four-byte prefix plus binary protobuf payload in lowercase `.frame.hex`, and an exhaustive renderer derives the deterministic, project-readable `.json` semantic companion from the same value. The JSON is not claimed to be canonical protobuf JSON. Generation round-trips every typed frame and rejects a semantic companion that disagrees with it. Generated source headers name the authoritative inputs and do-not-edit owner. `cargo xtask generate-check` and `cargo xtask protocol verify` own drift, descriptor/package/message/signature/tag, policy-route, cap, and fixture-inventory checks.

The handshake-success fixture uses the exact semver `9.15.6755`. A later source-archive native worker build must set `OR_TOOLS_PATCH=6755`; without `.git` metadata, the upstream archive CMake logic otherwise defaults the patch component to `9999`, which would fail version negotiation.
