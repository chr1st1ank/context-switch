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

Typing the identifier on every span is slow, and one typo silently creates
its own group in the totals.

## Decision Drivers

- Silent data loss in a synchronized system is worse than a forced upgrade.
- Remapping time for billing must not change how it is tracked internally.
- Setting the identifier once per project must cover the common case.
- Picking a value must be faster than typing it, and a misspelling must be
  impossible rather than merely unlikely.

## Considered Options

- A free-text string on the project and the span, with suggestions built
  from values already in use.
- A first-class client project record that projects and spans point to by
  ID, on both, with the span value overriding the project value.
- The same, plus a per-project shortlist of allowed client projects.

## Decision Outcome

Chosen: **a first-class client project record, referenced by ID from
projects and spans, span overriding project**, with a bump to **schema v3**.

- `ClientProject` has `id`, `name`, `archived` and timestamps, and lives in
  `Logbook.client_projects`. Names are case-insensitively unique, including
  archived records, like projects and tags.
- `Project.client_project_id` and `Span.client_project_id` are optional
  UUIDs, serialized only when present. `validate` rejects dangling IDs.
- A span's effective client project is its own, else its project's current
  one, else none (`Logbook::effective_client_project`). Changing a
  project's value changes every span without an override; clearing a
  span's override makes it inherit again.
- Renaming a client project is one mutation and every project and span
  follows. Archiving hides it from pickers while history still resolves.
- Remapping a span never touches `project_id`; internal reports stay
  unchanged.
- Clients pick from the list and never create a client project as a side
  effect. Unlike projects and tags, `cosw` does not auto-create one when a
  name is first used: it fails and points at `cosw client-projects add`,
  because auto-creation would bring the typo problem back. Android shows
  chips ordered by how many spans each client project covers.
- `cosw export --by-client-project` groups totals by effective client
  project. Spans without one fall into a single group with an empty label.
- `resume` copies the source span's override; `start` and `switch` begin
  without one.
- `Logbook::migrate` upgrades v1 and v2 to v3 unchanged apart from the
  version stamp. A v2 client reading a v3 logbook fails with
  `UnsupportedSchemaVersion` rather than dropping the data.
- The UniFFI `edit_span` takes a keep/set/inherit `ClientProjectEdit`,
  like `ProjectEdit`.

### Consequences

- Good: no typos, one rename fixes every span, and most spans need no
  value at all.
- Good: no extra schema bump. v3 had not reached any production logbook
  when this representation replaced an earlier free-text one.
- Bad: all devices must be upgraded together again; the first v3 commit
  cannot be undone by downgrading a client.
- Bad: a new customer identifier must be created before use, one extra
  step on first use.
- The effective value depends on the project's current value, so
  historical exports change when a project is pointed at another client
  project. Spans that must keep an old identifier need their own override.
- A per-project shortlist was not built; it can be added later without
  changing the format.

See ADR-0013 for the migration policy this extends.
