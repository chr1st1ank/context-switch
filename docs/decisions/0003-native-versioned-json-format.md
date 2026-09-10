---
status: "proposed"
date: 2026-09-10
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Use a native versioned JSON format

## Context and Problem Statement

The system needs a portable native format for mutable spans, optional projects, stable identities for projects and tags, versioned synchronization, offline conflict capture, and future exports in formats not yet selected.

Should the system use a positional tuple format or define its own canonical representation?

## Decision Drivers

- Clear schema evolution.
- Explicit support for unassigned time.
- Stable identities independent of display names.
- Conditional synchronization metadata.
- Human-readable local storage.
- Future export flexibility without coupling the domain to an output format.

## Considered Options

- Adopt a positional tuple format as the native format.
- Define a native versioned JSON object format.
- Use an append-only operation log as the only canonical format.

## Decision Outcome

Chosen option: "Define a native versioned JSON object format", because named fields and explicit IDs support the domain and synchronization requirements while leaving room for schema evolution.

### Consequences

- Good, because schema changes can add named optional fields and explicit migrations.
- Good, because `project_id: null` represents unassigned time directly.
- Good, because projects, tags, and spans can be renamed or edited without identity ambiguity.
- Good, because exporters can target future formats from a stable internal model.
- Bad, because compatibility with external legacy formats is not automatic and is explicitly out of scope.
- Bad, because schema migrations and compatibility tests become a project responsibility.
- Neutral, because JSON remains easy to inspect and back up, while the storage envelope can carry revisions.

## Implementation Plan

- **Affected paths**: future schema definitions, domain serialization/deserialization, migration registry, provider document envelope, conformance fixtures, report/export modules.
- **Dependencies**: select a JSON Schema validator appropriate to each implementation language; do not make a schema library part of the domain model.
- **Patterns to follow**: named fields, stable IDs, explicit `schema_version`, UTC timestamps, nullable `project_id`, record/document revision metadata.
- **Patterns to avoid**: positional tuples as the canonical format; project names as identity; report-specific persisted fields; choosing an export format before the native model is stable.
- **Configuration**: supported schema versions and migration policy.
- **Migration steps**: define schema version 1; add fixtures for active, stopped, unassigned, tagged, edited, and conflicted captures; implement forward migrations before introducing schema version 2.

## Verification

- [ ] A valid document can represent an active timer, a stopped span, unassigned time, projects, and multiple tags.
- [ ] A project rename does not change historical span identity or classification.
- [ ] Serialization round-trips without loss of domain data.
- [ ] Schema validation rejects malformed or unsupported versions clearly.
- [ ] Reports operate from the native raw records rather than persisted report state.
- [ ] Future exporters can consume the native model without changing synchronization semantics.
