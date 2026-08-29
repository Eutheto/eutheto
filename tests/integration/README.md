<!-- SPDX-License-Identifier: Apache-2.0 -->

# Integration tests

Reserved for tests that exercise real cooperation across implemented Rust crates, application services, persistence, adapters, or generated API boundaries. A case belongs here only when a package-local test cannot prove the observable contract; it must use real implemented components rather than a fake authority that makes an incomplete feature appear functional.

Future fixtures must declare deterministic clock/time zone/locale/seed/thread/temp context and verify that a nonzero case set executed. Phase 00 contains no integration scenario or passing integration fixture.
