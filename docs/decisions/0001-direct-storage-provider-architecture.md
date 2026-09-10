---
status: "proposed"
date: 2026-09-10
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Use direct clients with a storage-provider abstraction

## Context and Problem Statement

The context-switch system must support the `cosw` CLI, Android app, and reporting dashboard for one user. The user wants central data reachable from every device, with local filesystem storage as the simplest provider and remote object storage as the intended shared provider. An API server is not part of the initial scope.

How should clients access synchronized data without coupling the domain to one storage product?

## Decision Drivers

- No API server in the initial architecture.
- Local and remote storage must be usable through one client-facing contract.
- The single global timer invariant must be enforceable centrally.
- Provider credentials and optional encryption must remain infrastructure concerns.
- The system should support only a small, deliberate provider set rather than optimize for provider count.

## Considered Options

- Direct clients through a storage-provider abstraction.
- Central synchronization/API service.
- Client-specific direct storage implementations.

## Decision Outcome

Chosen option: "Direct clients through a storage-provider abstraction", because it satisfies the no-server scope while allowing local filesystem and remote object storage to implement one consistency contract.

### Consequences

- Good, because `cosw`, the Android app, and the dashboard can use the same logical storage contract.
- Good, because local storage can be used for development, backup, and tests.
- Good, because adding a server later remains possible behind the same logical interface.
- Bad, because direct clients must handle provider authentication, retries, cache state, and conflict presentation.
- Bad, because object-storage transaction semantics must be designed explicitly; a generic blob API is not enough.
- Neutral, because the initial system remains single-user and does not provide server-side access control.

## Implementation Plan

- **Affected paths**: future client core modules, `storage` provider interface, local-file provider, remote-object provider, client configuration, credential adapters, integration tests.
- **Dependencies**: no dependency selected by this ADR. Select a mature SDK only after the remote provider is chosen.
- **Patterns to follow**: keep provider authentication, encryption, and serialization adapters outside domain rules; expose snapshots plus conditional commits through the provider interface.
- **Patterns to avoid**: direct object-storage SDK calls from UI/client workflows; provider-specific types in domain entities; an API server introduced only to hide an underspecified provider contract.
- **Configuration**: provider kind, provider location/bucket/path, user credentials, optional encryption settings.
- **Migration steps**: implement local storage first; add one remote provider conforming to the same contract; a future API service may become another provider without changing domain behavior.

## Verification

- [ ] `cosw`, Android, and dashboard workflows depend only on the provider interface.
- [ ] Local and remote providers pass the same provider conformance suite.
- [ ] No domain module imports a provider SDK or stores provider credentials.
- [ ] A provider lacking conditional-write support is rejected during configuration or capability detection.
- [ ] A future API-backed provider can be introduced without changing span rules.
