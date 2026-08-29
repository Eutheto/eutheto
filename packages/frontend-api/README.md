<!-- SPDX-License-Identifier: Apache-2.0 -->

# `@eutheto/frontend-api` (reserved)

Future generated application DTOs plus a narrow frontend transport facade. This directory intentionally has no `package.json`, so it is not an installed pnpm workspace project, publishable package, or implemented API.

## Boundary

Only the desktop API boundary may import Tauri invoke/event primitives. Generated contracts must retain a single authoritative source and must not be hand-edited here.

The owning roadmap phase must decide whether this responsibility still needs a separate package, then add a private/public decision, dependencies, implementation, and tests. The exact package inventory remains an explicit gate; this reserved npm-style name does not freeze it.
