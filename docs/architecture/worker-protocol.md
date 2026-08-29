<!-- SPDX-License-Identifier: Apache-2.0 -->

# Solver worker protocol

[`protocol/solver-worker.proto`](../../protocol/solver-worker.proto) is the authoritative Phase-00 worker boundary. It defines only version negotiation, worker identity, manifest binding, and health. It has no planning IR, model, solve, solution, progress, cancellation, or domain semantics, and there is no protocol peer or generated binding in Phase 00. The matched protobuf generator/runtime contract closes with the real OR-Tools worker in Phase 03; Phase 00 does not guess it.

The current wire version is the integer `1` in [`protocol/version.json`](../../protocol/version.json). Writers emit only version 1 and readers accept exactly version 1. A semantic change to a field, enum value, lifecycle, or framing rule requires a new wire version. A removed field number or enum value is reserved forever and is never reused with another meaning.

## Transport envelope

Each frame on the worker byte stream is:

1. a four-octet unsigned big-endian payload length; then
2. exactly that many octets of binary protobuf `SolverWorkerFrame`.

The length counts only the protobuf payload. A receiver reads the prefix first and rejects zero or more than 65,536 payload octets before allocating or decoding the body. Truncated prefixes, truncated payloads, trailing payload bytes, invalid protobuf, absent/multiple oneof bodies, and invalid UTF-8 strings are malformed frames. There is no compression and no resynchronization after malformed input; the receiver terminates the session.

`request_id` is a correlation token, not a domain identifier. A request uses a non-empty token unique within the worker session; its result or error echoes that token. The one exception is an unknown-version error, which leaves the token absent because fields from an unknown envelope are not trusted.

## Phase-00 lifecycle

A session has at most 32 frames in both directions combined:

1. the supervisor sends exactly one `HandshakeRequest` as the first request;
2. the worker returns exactly one matching `HandshakeResult` or `WorkerError`;
3. only after a successful handshake may the supervisor send `HealthRequest` values; and
4. each health request receives exactly one matching `HealthResult` or `WorkerError`.

Duplicate handshakes, health before handshake, repeated request IDs, unsolicited results, a second terminal response, and unknown request kinds fail the session. `HEALTH_STATE_READY` means only that this future process has completed this handshake and can answer the bounded health surface. It is not evidence that OR-Tools is present, loadable, licensed, or capable of solving. Phase 00 ships no worker that can report it.

A handshake result contains:

- `worker_identity`, the executable contract identity (`eutheto-ortools-worker` for the planned official worker);
- `worker_version`, the project worker version, not the upstream OR-Tools version;
- `manifest_sha256`, exactly 64 lowercase hexadecimal characters binding the canonical installed solver manifest; and
- at most 16 unique, nonzero capabilities. Version 1 defines only `WORKER_CAPABILITY_HEALTH`.

An adapter must compare all expected handshake values before considering the worker compatible. Identity, version, manifest, protocol, or capability mismatch is a typed incompatibility and the process is terminated. It is never converted into an empty or successful solve result.

## Limits

All byte limits apply after transport decoding; string limits count UTF-8 octets, not Unicode scalar values.

| Item | Maximum |
|---|---:|
| protobuf payload in one frame | 65,536 bytes |
| frames in one session, both directions combined | 32 |
| message nesting depth | 4 |
| `request_id` | 64 bytes |
| worker identity | 128 bytes |
| worker version | 64 bytes |
| manifest SHA-256 text | exactly 64 bytes |
| capabilities in one handshake | 16 |
| error message | 512 bytes |

`request_id` is restricted to ASCII letters, digits, `.`, `_`, and `-`. Identity and version are printable UTF-8 without controls or line breaks. Error text is fixed/bounded diagnostic text: it must not include user data, credentials, environment values, arbitrary paths, or a reflected malformed payload. Count and size arithmetic is checked before addition or allocation. A resource-limit violation uses `WORKER_ERROR_CODE_RESOURCE_LIMIT` only when a bounded error can be sent safely; otherwise the process closes the stream.

## Version and error behavior

The protocol version is checked before dispatching the body. For a well-framed envelope carrying any version other than 1, a version-1 worker sends at most one bounded `WorkerError` with:

- frame `protocol_version = 1`;
- no `request_id`;
- code `WORKER_ERROR_CODE_UNSUPPORTED_PROTOCOL_VERSION`;
- fixed message `unsupported protocol version`; and
- `rejected_protocol_version` set to the received integer;

then closes the stream. It must not interpret or execute the unknown body. A version-1 supervisor treats an unknown worker version identically as an incompatible worker and closes it. Malformed input for which the version cannot be read safely is closed without a response. Errors never trigger error-on-error loops.

Within version 1, enum zero values, unknown enum values, unknown body variants, and unexpected protobuf fields are rejected rather than silently treated as a known capability or request. The optional `rejected_protocol_version` field is present only for an unsupported-version error, including when the rejected value is zero; it is absent for every other error code. Internal errors disclose only the fixed bounded category.

## Stable identifiers

The declarations below are the complete version-1 tag/value allocation. Names may improve in documentation, but these numbers and meanings cannot change in place.

| Declaration | Stable allocation |
|---|---|
| `SolverWorkerFrame` | `protocol_version = 1`, `request_id = 2`, `request = 3`, `result = 4`, `error = 5`; 6–15 reserved |
| `WorkerRequest` | `handshake = 1`, `health = 2`; 3–15 reserved |
| `HandshakeRequest`, `HealthRequest` | 1–15 reserved |
| `WorkerResult` | `handshake = 1`, `health = 2`; 3–15 reserved |
| `HandshakeResult` | `worker_identity = 1`, `worker_version = 2`, `manifest_sha256 = 3`, `capabilities = 4`; 5–15 reserved |
| `WorkerCapability` | unspecified `0`, health `1` |
| `HealthResult` | `state = 1`; 2–15 reserved |
| `HealthState` | unspecified `0`, ready `1` |
| `WorkerError` | `code = 1`, `message = 2`, `rejected_protocol_version = 3`; 4–15 reserved |
| `WorkerErrorCode` | unspecified `0`, malformed frame `1`, unsupported protocol version `2`, resource limit `3`, unsupported request `4`, internal `5` |

## Golden fixtures

[`protocol/golden`](../../protocol/golden) contains paired canonical protobuf-JSON and `.frame.hex` files for handshake and health requests/results plus an unknown-version request and its bounded error. Each hex file is lowercase, has no separators, ends with one LF, and represents the complete four-byte prefix plus protobuf payload. The JSON form uses protobuf JSON field/enum names, two-space indentation, lexicographically ordered object keys, UTF-8, and one trailing LF.

The binary fixtures encode fields once in ascending tag order, omit default scalar values, preserve the selected empty-message oneof as a zero-length embedded message, and use no map fields. The handshake result's worker version and manifest digest are fixture-only values; they are not an OR-Tools bundle or release claim. A locked generator in Phase 03 must reproduce these exact octets before generated bindings or a worker may be accepted.
