<!-- SPDX-License-Identifier: Apache-2.0 -->

# eutheto governance

## Purpose and present state

This document defines how repository changes and project decisions are reviewed. `eutheto` has entered Phase 02, domain-pack and planning-IR contracts. Governance must not be used to waive phase prerequisites, security invariants, compatibility obligations, or exit evidence, and it must not imply that a later product capability or a public release exists.

The [roadmap index](docs/roadmap/README.md) and [active Phase 02 document](docs/roadmap/02-domain-pack-and-planning-ir-contracts.md) are binding. The authority order is:

1. approved security and data-integrity ADRs;
2. current schemas and conformance tests;
3. the roadmap index, active phase, and domain specifications;
4. generated documentation;
5. code comments.

When authorities conflict, the higher authority controls. The project changes the authoritative source and its validation together; a review vote cannot silently create an exception.

## Roles

Participation is based on contribution and responsibility, not employment, affiliation, or financial support.

- **Contributors** propose code, documentation, design, tests, research, or review. Any participant whose contribution is accepted is a contributor.
- **Reviewers** provide evidence-based review in areas they understand. Review alone does not grant merge or decision authority.
- **Maintainers** are contributors named by an approved governance record, granted repository merge rights, and assigned explicit ownership areas. They triage work, protect roadmap and architecture boundaries, require appropriate review, and keep automation and documentation honest.
- **Code owners** are maintainers or designated reviewers recorded in `CODEOWNERS` for sensitive paths. Ownership means required review responsibility, not unilateral authority over unrelated areas.
- **Release stewards** and **security responders** are separately designated roles with least-privilege access. They do not exist merely because someone is a maintainer, and their appointment does not itself resolve signing identity or private-contact gates.

The initial maintainer roster and governance contact remain unresolved named roadmap gates. This document names no person, organization, or address by implication.

- **Owner:** the governance and public-identity decisions required by the roadmap.
- **Closure criterion:** an approved ADR names consenting initial maintainers, their ownership areas, appointment/removal rules, the canonical governance contact, required private channels, and the repository records those decisions consistently.

Until that gate closes, existing approved roadmap decisions remain authoritative, but no new proposal may be represented as a project-wide governance decision merely because it appears in a branch or receives informal agreement.

## Contribution and review process

1. A proposal identifies the active roadmap work package, acceptance criterion, relevant ADRs, and affected trust or compatibility boundaries.
2. The author supplies a complete, focused change with the narrow verification that proves its claims. Generated outputs come from authoritative inputs and canonical generation commands, never hand edits.
3. Reviewers assess correctness, scope, architecture, security and privacy, data and protocol compatibility, accessibility where applicable, dependency and asset licensing, tests, and documentation.
4. Required code owners review changes in their areas. Security-sensitive, release, credential, parser, protocol, Tauri-capability, signing/updater, and license-policy changes require explicit specialist ownership review as established by repository policy.
5. The author addresses material findings. Disagreement is resolved against repository evidence and authority, not seniority or volume.
6. A maintainer with merge authority confirms approvals, required checks, DCO sign-offs, and phase fit before merge. Passing automation is necessary where required but never waives human review or an open gate.

Authors and reviewers must disclose a material personal, employment, financial, or other conflict that could reasonably affect judgment. A conflicted decision-maker recuses from the deciding approval. If no unconflicted authorized decision-maker exists, the change remains unapproved.

## Decision classes

### Routine implementation decisions

A change that implements an already approved contract without changing public behavior, architecture, security posture, data meaning, compatibility, licensing, identity, or governance follows normal review. Maintainers seek rough consensus: material objections are resolved or explicitly answered from controlling evidence before merge.

### Contract and cross-cutting decisions

Changes to a public schema or protocol, compatibility policy, dependency direction, security boundary, generated-code authority, canonical command contract, license policy, supported platform, or phase exit evidence require review from every affected ownership area. If the change modifies or contradicts a durable decision, it also requires an ADR.

### Emergency decisions

A credible vulnerability or active integrity risk may justify a private, minimal mitigation before ordinary public discussion. Emergency authority is limited to designated, unconflicted security responders and maintainers, once those roles and the private channel exist. The mitigation must preserve evidence, minimize unrelated change, receive retrospective review, and be documented publicly only when coordinated disclosure makes that safe. Emergency handling cannot be used to bypass the unresolved security-contact gate or to conceal non-security decisions.

## ADR process

Use an Architecture Decision Record for a decision that:

- changes or supersedes an approved ADR;
- changes architecture or dependency direction;
- creates or changes a security, privacy, data-integrity, or trust boundary;
- freezes a public identifier, schema, protocol, compatibility promise, extension, or media type;
- selects a solver, material dependency family, licensing exception, signing model, or release channel;
- establishes governance roles, contacts, enforcement, or decision authority;
- cannot satisfy a roadmap invariant or stop condition without an explicit decision.

An ADR contains a numbered title, status, date, context, decision, considered alternatives, consequences, security/data/compatibility implications, affected roadmap gates, and links to any record it supersedes. Proposed ADRs are not authority. Discussion should be public by default; vulnerability details, credentials, personal conduct reports, signing custody, and other sensitive material stay in the appropriate private process and are summarized only when safe.

Acceptance requires the reviewers and code owners for every affected area, an unconflicted maintainer decision, and updates to the roadmap, tests, schemas, or policies that the decision changes. An accepted ADR is immutable historical evidence: later changes add a superseding ADR and links rather than rewriting the old decision. If required authority has not yet been appointed, the ADR remains proposed.

## Resolving disagreement and appeals

Participants first identify the concrete disputed claim and the controlling evidence. The author and reviewers should seek the smallest change that satisfies the contract. If disagreement remains, the relevant code owners provide written analysis and an unconflicted maintainer records one of these outcomes: accept, request revision, reject with reasons, or require an ADR.

A contributor may request reconsideration by an unconflicted maintainer not responsible for the original decision. The request must identify overlooked evidence, a process failure, or a conflict with higher authority; it is not a repeated vote. If the roster cannot supply an unconflicted maintainer, no final appeal is available and the change remains unmerged until the governance gate supplies one.

Conduct enforcement follows [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md), and vulnerability handling follows [SECURITY.md](SECURITY.md). Neither sensitive process belongs in a public dispute thread.

## Appointment, inactivity, and removal

After the initial-roster ADR closes the gate, new maintainers are appointed through an ADR or the process that ADR establishes. Selection must be based on sustained, constructive contribution, sound judgment in the proposed ownership area, reliable review, security and licensing care, and explicit consent.

The same approved process must define inactivity review, voluntary resignation, access revocation, emergency suspension, and removal for conduct or trust failures. Repository, release, security, and signing privileges follow least privilege and are removed promptly when the associated responsibility ends. No role is permanent by default, and no project title grants ownership of contributors' work or authority beyond its recorded scope.

## Amending governance

A governance amendment requires an ADR, public review except for narrowly necessary sensitive evidence, approval from the governance ownership defined by the closed roster decision, and synchronized updates to affected policy and access records. An amendment cannot retroactively legitimize a decision made without the authority required at the time.