<!-- SPDX-License-Identifier: Apache-2.0 -->

# Security policy

## Current support status

`eutheto` is in Phase 00, repository and reproducible tooling. There is no public product release and therefore no supported release series, security-update branch, solver service, updater, hosted account system, or production security response commitment. Source on the current development branch is bootstrap work, not a supported binary.

Security and data integrity remain release constraints now. The [roadmap index](docs/roadmap/README.md), [active Phase 00 document](docs/roadmap/00-repository-and-reproducible-tooling.md), and approved security/data-integrity ADRs govern work in that order of authority described there. Phase gates cannot be waived by labeling incomplete behavior experimental.

When releases become supported, this section must list exact supported versions, update channels, and end-of-support rules. Until then, do not infer support from a tag, artifact, branch, package name, or local build.

## Reporting a vulnerability: private-channel gate

The canonical hosting organization, repository URL, private security-reporting contact, and accountable security responders are unresolved roadmap gates. The project will not invent an email address or direct reports to an unmonitored recipient.

- **Owner:** the Phase 00 security and governance decisions required by the roadmap.
- **Closure criterion:** an approved ADR identifies accountable responders and a monitored private intake channel; defines access, backup coverage, conflicts and recusals, encryption and retention expectations, acknowledgment and coordination policy; and updates this file at the canonical repository with exact instructions.

**Do not publish a suspected vulnerability.** Do not place vulnerability details, proof-of-concept code, affected data, credentials, logs, screenshots, or exploit steps in a public issue, discussion, pull request, commit, chat room, or social-media post. A public request for someone to contact you can itself reveal timing or target information and is not the project's reporting process.

While the private-channel gate is open:

1. stop testing once you have enough information to recognize a credible issue;
2. do not access, change, retain, or share data that is not yours, and do not test third-party or production systems;
3. preserve only the minimum notes needed for a later report in a private, access-controlled location; remove secrets and unrelated personal data;
4. do not send the report to guessed addresses, individual contributors, or an unrelated conduct channel;
5. wait for this file at the canonical repository to publish the approved private channel before sending project vulnerability details.

If there is an immediate threat to people, systems, or data outside this repository, use the affected platform or service's official security or emergency process without disclosing `eutheto` vulnerability details publicly. These interim instructions are harm-reduction guidance, not a functional project intake channel and not a promise that the project has received a report.

## What a future private report should contain

After the private channel is published, provide only information you are authorized to share:

- the affected component, revision, version, and platform;
- the security impact and realistic threat model;
- minimal, reproducible steps or a minimal proof of concept;
- whether the issue is already known or disclosed elsewhere;
- mitigations you have tried and any suspected regression range;
- redacted logs or artifacts when essential;
- a safe way for the response team to continue private coordination.

Never include live credentials, signing material, unrelated user data, full local databases, captured user scenarios, or unsanitized support bundles. Use synthetic data where possible. The response team may request clarification but will not require access to third-party data as proof.

## Coordinated handling after the gate closes

The approved response process must:

1. acknowledge receipt through the private channel without promising a conclusion;
2. restrict details to unconflicted people needed to assess and remediate the issue;
3. reproduce safely and determine affected supported versions and artifacts;
4. fix the causal mechanism and verify request and response boundaries, redaction, and regression behavior;
5. prepare supported-version updates, dependency or signing actions, and user mitigation as applicable;
6. agree on disclosure timing with the reporter when possible, while prioritizing user safety and legal obligations;
7. publish an advisory with accurate impact, affected versions, fixes, credit preference, and residual risk only when users can act safely.

No bounty, response deadline, confidentiality agreement, embargo duration, or credit outcome exists until explicitly adopted. The project will not describe a backend claim, passing build, or unavailable exploit as proof that a vulnerability is resolved.

## Security boundaries for contributors

Contributions must preserve the security and privacy rules in `AGENTS.md` and the roadmap, including:

- secrets never enter Vue/JavaScript state, logs, SQLite, exports, diagnostics, Nix derivations, repository files, caches, pull-request jobs, or ordinary IPC payloads;
- credentials use a Rust/native-owned secure entry surface and the operating-system credential store; frontend code receives only opaque references and status;
- no telemetry or network access is enabled by default, and future providers remain optional and explicit;
- Tauri commands, capabilities, CSP, filesystem access, CI tokens, and release environments use least privilege;
- imported files, bundles, worker frames, provider output, and URLs are untrusted, bounded, parsed, and validated before an atomic commit;
- solver workers remain isolated from the desktop process; worker failure cannot corrupt authoritative scenario state;
- every candidate remains untrusted until projection, independent domain verification, and authoritative score recomputation pass;
- AI cannot bypass typed validation, mutate persistence directly, access arbitrary files, execute shell or code, or become solver/verifier authority;
- hidden global mutable state is forbidden, and any narrow `unsafe` exception requires an isolated safety module, documented invariants, dedicated tests, and maintainer review;
- dependencies, install scripts, parsers, cryptography, updater/signing code, worker protocol, and Tauri permission changes receive designated ownership review.

Never commit `.env` files, credentials, tokens, signing keys, notarization material, local databases, captured scenarios, or unsanitized diagnostics. If secret material is discovered in repository history, do not quote or copy it into a public report; treat it as a private security matter under the gate above. Removing a secret from the latest revision does not revoke it or remove history.

## Safe research expectations

Security research must use systems and data you own or have explicit permission to test. Avoid privacy violations, denial of service, persistence, destructive changes, social engineering, supply-chain publication, automated high-volume traffic, and accessing more data than needed to demonstrate impact. Stop when continued testing could harm another person or system.

A local Phase 00 checkout is the appropriate target for repository-level analysis. The project currently authorizes no testing of third-party infrastructure and makes no safe-harbor promise beyond rights already available under applicable law. Researchers remain responsible for obtaining authorization and complying with law.

## Public, non-sensitive hardening work

A normal public contribution may address defense-in-depth, dependency maintenance, tests, documentation, or hardening only when it does not reveal an unremediated vulnerability or private report. If explaining the change would expose an exploitable weakness, keep it out of public branches and wait for the approved private process. Ordinary quality bugs with no security impact follow [CONTRIBUTING.md](CONTRIBUTING.md).