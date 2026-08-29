<!-- SPDX-License-Identifier: Apache-2.0 -->

# Cross-cutting tests

This tree reserves repository-level suites whose scope crosses a crate or package boundary:

- [`integration/`](integration/) for cooperating application components;
- [`e2e/`](e2e/) for real user-facing executable surfaces;
- [`migration/`](migration/) for released-format compatibility;
- [`protocol/`](protocol/) for versioned protocol conformance; and
- [`security/`](security/) for trust-boundary and abuse cases.

Unit and package-local tests remain beside their owning implementation. All future cases follow the [test fixture contracts](../docs/architecture/test-fixtures.md). Phase 00 adds no case, ignored placeholder, fake executable, or passing claim; a suite that discovers zero tests is not acceptance evidence.
