<!-- SPDX-License-Identifier: Apache-2.0 -->

# `@eutheto/test-fixtures` (reserved)

Future shared synthetic frontend test inputs and helpers. This directory intentionally has no `package.json`, so it is not an installed pnpm workspace project, publishable package, or implemented API.

## Boundary

Production packages must not depend on it. Inputs must be deterministic, bounded, sanitized, and must follow the authoritative generated DTO/schema versions once those contracts exist.

The owning roadmap phase must decide whether this responsibility still needs a separate package, then add a private/public decision, dependencies, implementation, and tests. The exact package inventory remains an explicit gate; this reserved npm-style name does not freeze it.
