<!-- SPDX-License-Identifier: Apache-2.0 -->

# End-to-end tests

Reserved for tests of real executable user surfaces, including the packaged Tauri application when behavior depends on native integration. Browser-only rendering tests do not substitute for packaged desktop evidence, and a mock service or no-op executable does not establish an end-to-end path.

Future cases must use synthetic data, isolated temporary state, explicit clock/time zone/locale/seed/thread context, and observable accessibility and persistence outcomes as applicable. Phase 00 contains no domain journey, scenario, or passing E2E fixture.
