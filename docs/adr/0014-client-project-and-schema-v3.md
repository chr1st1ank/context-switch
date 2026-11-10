---
status: "accepted"
date: 2026-11-10
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Client project and logbook schema v3

## Context and Problem Statement

Customers track work under their own identifiers, which rarely match the
internal project names. Timesheets sent to a customer need totals per
customer identifier. Most spans of a project map to the same identifier,
but some work under one internal project is billed elsewhere. As with span
descriptions (ADR-0013), a client that does not know a new field drops it
on its next whole-logbook write.

## Decision Drivers

- Silent data loss in a synchronized system is worse than a forced upgrade.
- Remapping time for billing must not change how it is tracked internally.
- Setting the identifier once per project must cover the common case.

## Considered Options

- Client project on the project only.
- Client project on the span only.
- On both: the span value overrides the project value.

## Decision Outcome

Chosen: **on both, span overrides project**, with a bump to **schema v3**.

- `Project.client_project` and `Span.client_project` are optional strings,
  serialized only when present; trimmed, blank becomes `None`, no length
  limit.
- A span's effective client project is its own value, else its project's
  current value, else none (`Logbook::effective_client_project`).
  Changing a project's value changes every span without an override;
  clearing a span's override makes it inherit again.
- Remapping a span never touches `project_id`; internal reports stay
  unchanged.
- `cosw export --by-client-project` groups totals by effective client
  project. Spans without one fall into a single group with an empty label.
- `resume` copies the source span's override; `start` and `switch` begin
  without one.
- `Logbook::migrate` upgrades v1 and v2 to v3 unchanged apart from the
  version stamp. A v2 client reading a v3 logbook fails with
  `UnsupportedSchemaVersion` rather than dropping the field.

### Consequences

- Good: one setting per project covers most spans; exceptions are
  per-span without distorting internal tracking.
- Bad: all devices must be upgraded together again; the first v3 commit
  cannot be undone by downgrading a client.
- The effective value depends on the project's current value, so
  historical exports change when the project's value is changed. Spans
  that must keep an old identifier need their own override.

See ADR-0013 for the migration policy this extends.
