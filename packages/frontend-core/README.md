<!-- SPDX-License-Identifier: Apache-2.0 -->

# `@eutheto/frontend-core` (reserved)

Future platform-neutral frontend presentation logic and state adapters. This directory intentionally has no `package.json`, so it is not an installed pnpm workspace project, publishable package, or implemented API.

## Boundary

It may consume the frontend API contract, but Pinia/query state remains a cache; this boundary must not become a second authority for scenario state, validation, persistence, feasibility, or scoring.

The owning roadmap phase must decide whether this responsibility still needs a separate package, then add a private/public decision, dependencies, implementation, and tests. The exact package inventory remains an explicit gate; this reserved npm-style name does not freeze it.
