<!-- SPDX-License-Identifier: Apache-2.0 -->

# Protocol tests

Reserved for cross-component conformance tests of implemented, versioned protocols. Future worker-protocol cases must cover framing and limits, compatible negotiation, canonical encoding where specified, malformed and truncated messages, unknown fields/tags, unknown-newer refusal, cancellation and failure boundaries, and checked generated-artifact drift against the authoritative schema.

Golden inputs and expected outputs must identify their protocol and expected-result versions. Phase 00 does not fabricate a worker frame, backend response, solver capability, passing conformance fixture, or runnable worker.
