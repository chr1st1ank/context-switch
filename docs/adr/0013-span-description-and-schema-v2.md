---
status: "accepted"
date: 2026-10-20
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# Span description and logbook schema v2

## Context and Problem Statement

Spans record when and on what (project, tags) time was spent, but not what
was actually done. Users want an optional free-text note per span. Because
every commit rewrites the whole logbook, a client that does not know about
a new field silently drops it on its next write. Adding the field without
a coordinated schema change would therefore lose data whenever devices run
different versions.

## Decision Drivers

- Silent data loss in a synchronized system is worse than a forced upgrade.
- The system has a single user who controls every device.
- Validation rules must be identical in every client, so they live in the
  core.
- ADR-0004 states that changes needing a read-path migration bump
  `schema_version`.

## Considered Options

- Keep schema v1; add an optional field and let old clients ignore it.
- Bump to schema v2; old clients refuse to read the logbook.
- Keep v1 and make the model preserve unknown fields.

## Decision Outcome

Chosen: **bump to schema v2**, a deliberate breaking change.

- `Span.description` is an optional string, serialized only when present.
  It is trimmed, blank becomes `None`, newlines are allowed, and it is
  limited to 1000 characters; the core enforces this in
  `Logbook::set_span_description`.
- `resume` copies the description; `start`, `switch` and `add` begin
  without one unless given.
- `Logbook::migrate` is a pure, one-way upgrade applied on every provider
  read (plain and encrypted alike) and in `Logbook::from_json`. v1 is
  upgraded in memory and written as v2 on the next commit; anything newer
  than the library understands is rejected. `validate` only accepts the
  current version. Later bumps extend the same function.
- A v1 client reading a v2 logbook fails with `UnsupportedSchemaVersion`
  rather than overwriting it, so every device must be updated before the
  first v2 write.

### Consequences

- Good: no silent loss of descriptions; migration policy is established
  for future schema changes.
- Bad: all devices must be upgraded together; the first v2 commit cannot
  be undone by downgrading a client.
- The Android UI for viewing and editing descriptions is a follow-up; the
  UniFFI `SpanRec` already carries the field.

See ADR-0004 for the schema this extends.
