<!-- SPDX-License-Identifier: Apache-2.0 -->

# Git workflow

Use a short-lived branch and focused commits for one roadmap work package or independently reviewable change. Keep authoritative inputs, affected callers, tests, generated products, and contributor documentation in the same change. Do not combine an unrelated cleanup or dependency migration with the requested behavior.

## Before committing

1. Confirm that the change belongs to the active phase and preserves its prerequisites, non-goals, named gates, and acceptance criteria.
2. If a generated product changes, edit its authoritative input and run `just generate`; never hand-edit the product.
3. Run the narrowest canonical recipes that exercise the changed behavior. Run `just check` before proposing the branch unless a documented platform gate makes part of it inapplicable.
4. Inspect the patch for credentials, local databases, captured user data, caches, build output, and unsanitized diagnostics. None belong in Git.
5. Record new third-party code, assets, datasets, fonts, examples, and generated material in the applicable license inputs, then run `just licenses` and `just sbom`.

The root [`Justfile`](../../Justfile) and [command reference](commands.md) are authoritative. Do not replace a missing or gated recipe with an ad hoc command that appears successful.

## DCO sign-off

Every commit requires the contributor's own Developer Certificate of Origin sign-off:

```console
git commit -s
```

This adds a `Signed-off-by` trailer using the configured Git name and email. Read [`DCO.md`](../../DCO.md) before signing. Do not sign for another author. Correct every unsigned commit in the branch by amending or rebasing it before review; a sign-off added only to the final commit does not certify earlier commits.

## Optional local hooks

The repository includes small opt-in hooks in `.githooks/`. Enable them only for this checkout:

```console
git config --local core.hooksPath .githooks
```

Before changing the setting, inspect the effective hook path so that the checkout-local override does not unintentionally replace personal hooks:

```console
git config --get core.hooksPath
```

The hooks contain no independent policy or generation logic:

- `pre-commit` delegates to `just generate-check fmt-check`;
- `pre-push` delegates to `just check`.

They require `just` and the applicable pinned tools to be available, normally by working inside `nix develop` or the documented native environment. A hook failure blocks the local Git operation so the cause can be fixed; bypassing a hook does not bypass repository policy or required CI.

Disable the repository hooks with:

```console
git config --local --unset core.hooksPath
```

These hooks are a convenience, not an enforcement boundary. CI is authoritative and reruns canonical commands in pinned environments. Local hook success cannot waive CI, code-owner review, DCO, security, license, architecture, generated-drift, platform, or active phase gates.

## Dependency updates

Keep lockfiles committed and frozen outside an intentional dependency update. Dependency-update branches must preserve the exact Rust toolchain, Node/pnpm selection, flake lock, Cargo lock, pnpm lock, CI action SHAs, and generated evidence required by the roadmap.

Isolate a major Rust, Node, Tauri, OR-Tools, or schema migration as one independently reviewable major family. Do not combine major migrations into a rollup. New or changed JavaScript install scripts require explicit review and an allowlist rationale. Changes to cryptography, parsers or untrusted-input handling, credentials, updater/signing, worker protocol, or Tauri permissions require the designated ownership review.

## Push and pull request

Before pushing, make the branch reviewable:

- each commit is focused and DCO-signed;
- generated files have no unexplained drift;
- lockfile changes are intentional;
- tests and documentation describe only implemented behavior;
- failures caused by an unresolved platform or phase gate are reported rather than hidden;
- the pull request lists the exact commands run, the platform, results, and what those results prove.

Do not include vulnerability details in a public issue, commit, or pull request. Follow [`SECURITY.md`](../../SECURITY.md); if the required private reporting channel is still an unresolved gate, do not invent or infer a contact.

Review approval and passing CI do not override the active roadmap phase, an approved ADR, a compatibility contract, or a release/signing gate. Merge authority follows [`GOVERNANCE.md`](../../GOVERNANCE.md).
