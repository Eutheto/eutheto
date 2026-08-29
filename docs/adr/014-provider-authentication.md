<!-- SPDX-License-Identifier: Apache-2.0 -->

# ADR-014: Provider Authentication

- **Status:** Approved policy; implementation is deferred
- **Roadmap authority:** [Approved architecture decisions](../roadmap/README.md#approved-architecture-decisions)

## Context

Provider authentication varies, and unofficial consumer-session or OAuth mechanisms can violate provider terms, create insecure token custody, and fail without warning. MVP needs an explicit, supportable model without fabricating provider capabilities.

## Binding decision

> BYOK and local endpoints are MVP authentication; OAuth is allowed only when a provider officially supports suitable third-party authorization.

Bring-your-own-key credentials are stored only through the native credential boundary established by [ADR-010](010-local-state-and-credentials.md). Local endpoints remain explicit user configuration and do not imply trust in their output.

## Consequences

- Provider integration remains optional and off unless the user configures it.
- OAuth cannot be added from an unofficial consumer login flow or reverse-engineered session protocol.
- Any future OAuth implementation requires verified provider documentation, least scopes, secure redirect and token handling, revocation, expiry/refresh behavior, and OS credential-store custody.
- Phase 00 does not implement BYOK, endpoint access, OAuth, or provider catalogs.

## Rejected alternatives

- Unofficial consumer OAuth/session libraries and scraped browser sessions are rejected.
- Fabricating OAuth support for a provider without an official suitable third-party flow is rejected.
- Storing credentials in frontend state, SQLite, configuration exports, or repository `.env` files is rejected.

## Supersession

This decision remains approved until a later numbered ADR explicitly supersedes it. That ADR must link here, and this record must be updated to `Superseded` with the reciprocal link; edits must not silently change this decision.
