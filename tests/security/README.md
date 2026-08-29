<!-- SPDX-License-Identifier: Apache-2.0 -->

# Security tests

Reserved for repository-level tests at implemented trust boundaries: untrusted imports and archives, protocol frames, filesystem and temporary-state handling, desktop command/capability/CSP scope, credentials and redaction, dependency policy, and release artifact handoff. Future cases must exercise both accepted and rejected paths, enforce centralized bounds, and confirm that secrets, sensitive paths, and unsanitized data do not enter logs, exports, diagnostics, fixtures, or reports.

Adversarial fixtures must be synthetic, versioned, minimal, and safe to inspect or redistribute. Phase 00 contains no fabricated exploit corpus, credential, security result, or passing fixture.
