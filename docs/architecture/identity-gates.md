<!-- SPDX-License-Identifier: Apache-2.0 -->

# Identity Gates

## Status and authority

The project name `eutheto`, Rust crate prefix `eutheto-*`, npm scope `@eutheto/*`, and project-owned media namespace `eutheto/...` are resolved. The identities below are deliberately unresolved. The [roadmap authority](../roadmap/README.md#authority-and-unresolved-product-gates), [Phase-00 contract](../roadmap/00-repository-and-reproducible-tooling.md#decisions-and-invariants), and [assumptions ledger](../roadmap/assumptions.md#final-and-unresolved-identity-decisions) prohibit deriving or publishing placeholder values.

Phase 00 owns keeping each gate visible and preventing accidental registration. Phase 11 owns the final decision, ADR, configuration, and candidate evidence unless a row says otherwise. Phase 12 verifies the closure evidence against the exact candidate digests before public release. A value is not closed merely because it appears in example code, a local package configuration, a certificate request, or a test fixture.

## Open gates

| Open identity gate | Closure owner | Required closure evidence |
|---|---|---|
| CLI executable name | Phase 11, before final CLI reference, shell integration, or packaging | A numbered ADR selects the final name; collision and platform packaging checks are recorded; Cargo/Nix/Tauri packaging, command examples, completion/man-page inputs, release manifests, and tests use one value; the provisional `optimizer` label is removed or remains only in clearly historical text. |
| Reverse-domain stable application identifier | Phase 11 packaging/updater | A numbered ADR records the final identifier and controlling organization/domain rationale; Tauri/package metadata, platform bundle identifiers, installer/update configuration, migration/upgrade tests, and release manifests agree; clean-machine install and upgrade evidence proves identity continuity. |
| Reverse-domain beta application identifier | Phase 11 packaging/updater | The ADR records a distinct beta identifier and its relationship to stable; beta and stable storage, installation, and updater metadata cannot cross channels accidentally; explicit channel-selection and clean-machine coexistence/upgrade evidence is recorded. |
| Portable project file extension | Phase 11 public packaging/documentation, building on Phase-01 format and migration evidence | A numbered ADR selects the extension after collision and platform-association review; the scenario/bundle schema and media type remain authoritative; import/export, association, upgrade, malformed-input, and round-trip fixtures pass; examples, docs, installers, and release manifests use the final value consistently. |
| Git hosting organization and canonical repository URL | Phase 11 public repository/release setup | A numbered ADR records the controlling organization and canonical immutable-source location; governance authority and maintainer access are established; source/archive, issue, security, release, provenance, and package metadata links are verified; redirects or repository moves have an explicit continuity plan. |
| Governance contact route | Phase 11 governance readiness | Authorized maintainers approve and control a published contact route; the route is exercised end to end, has custody and continuity coverage, and matches `GOVERNANCE.md`, contribution guidance, CODEOWNERS/review policy, and release documentation. No unverified address or account closes this gate. |
| Private security-reporting route | Phase 11 security readiness; Phase 12 security/legal review | Authorized security responders control a non-public reporting route; end-to-end receipt, acknowledgement, access control, backup coverage, escalation, disclosure, and rotation procedures are exercised; `SECURITY.md` and release documentation agree. A public issue tracker alone does not close this gate. |
| macOS signing and notarization identities | Phase 11 protected signing; Phase 12 platform approval | The selected Developer ID/notarization identity, authorized team, custody, protected environment, rotation/revocation plan, and hardened-runtime entitlements are reviewed; the app and bundled worker are signed as required; notarization, stapling, Gatekeeper, and exact-digest verification pass on clean supported systems. |
| Windows code-signing identity | Phase 11 protected signing; Phase 12 platform approval | The selected Authenticode identity and timestamping arrangement, authorized custodians, protected signing service/environment, rotation/revocation response, and subject continuity are recorded; exact application, installer, and applicable worker digests verify after signing and timestamping on clean supported systems. |
| Linux release signing or attestation identity | Phase 11 protected release pipeline; Phase 12 platform approval | The selected signature and/or attestation trust identity, verification method, custody, rotation/revocation policy, and protected environment are recorded; checksums, provenance, packages, and any future repository metadata verify against exact published digests. |
| Updater signing identity and stable/beta trust roots | Phase 11 updater implementation; Phase 12 update gate | The updater public trust configuration, private-key custody, endpoint ownership, stable/beta separation, rotation/revocation/recovery plan, and identifier continuity are documented and tested; invalid, cross-channel, replayed, or mismatched metadata/packages are rejected before install. No endpoint or updater key is enabled by this document. |
| Release provenance/workflow identity and protected environments | Phase 11 release pipeline; Phase 12 approval | A numbered ADR or release-security decision identifies the trusted workflow/attestation principal and authorized approvers; immutable action pins, least privilege, protected-environment rules, digest handoff, signer verification, and provenance verification are exercised against the candidate artifacts. |

The signing rows may be resolved by one coherent release-signing ADR, but that ADR must identify each platform and channel decision explicitly; a generic statement that releases will be signed is insufficient.

## Working labels are not identities

`optimizer` is only the provisional CLI name. `.optplan` is only a working file-extension example. Neither may appear as a final association, updater identity, public compatibility promise, certificate identity, installer identity, or canonical URL. Tests and internal examples using a working label must mark it as provisional and keep replacement localized.

The project must not invent a reverse-domain prefix from `eutheto`, assume a hosting organization, fabricate an email address, publish an unowned endpoint, generate a placeholder signing key, or use a self-signed development certificate as release evidence.

## Closure procedure

Closing any gate requires one change set to:

1. add an approved numbered ADR before the identity's first public use;
2. update the assumptions ledger from open to closed with dated evidence;
3. update every authoritative configuration, manifest, generated artifact input, test fixture, example, and affected document;
4. remove provisional aliases and stale identifiers unless a published compatibility contract requires continued reading or migration;
5. add continuity, migration, collision, channel-separation, or custody tests appropriate to the identity; and
6. bind release-facing proof to the immutable Phase-11 candidate digests for Phase-12 review.

If the evidence is incomplete, the gate stays open. Phase 00 therefore contains no final value for any row above and makes no release, signing, updater, hosting, or contact-readiness claim.
