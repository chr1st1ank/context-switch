---
status: "proposed"
date: 2026-09-11
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# cosw CLI command surface

## Context and Problem Statement

`cosw` is the first client on the storage-provider contract and needed a
concrete command set. The owner is a daily Watson user, so Watson's CLI was
the obvious starting point — but the domain differs: stable project/tag
identities, a first-class switch operation, explicit `at` timestamps on every
mutation, and a glossary that avoids "restart" and "frame".

## Decision Drivers

- Command-line ergonomics for a daily driver; positional arguments beat flags.
- Domain language fidelity: commands say span/switch/resume, never
  frame/restart.
- One user action is one conditional commit; nothing client-specific leaks
  into the shared core.
- Output must be scriptable; machine-readable forms are required by the
  architecture.
- Destructive operations need a guardrail, but nothing interactive beyond a
  confirm prompt.

## Considered Options

- Watson-compatible surface adapted to the domain model.
- Strictly flag-driven interface (`--project`, `--tags`).
- Minimal lifecycle-only CLI with management deferred.

## Decision Outcome

Chosen option: "Watson-compatible surface adapted to the domain model".

### Commands

```text
cosw [--data-file PATH] COMMAND ...
├── start PROJECT [+TAG ...] [--at WHEN]
├── stop [--at WHEN]
├── switch [PROJECT] [+TAG ...] [--at WHEN]
├── resume [SPAN] [--at WHEN]        # copy classification of a previous span
├── cancel [-f]                      # discard the active timer
├── status [-j]
├── add PROJECT [+TAG ...] --from WHEN --to WHEN
├── edit [SPAN] [+TAG ...] [--start W] [--stop W] [--project N | --unassign] [--untag T]
├── remove SPAN [-f]
├── log    [filters] [-r] [-c|-C] [-j]
├── report [filters] [--by project|tag|day] [-c|-C] [-j]
├── projects [--all] [-j] | projects add|rename|archive|unarchive
└── tags     [--all] [-j] | tags     add|rename|archive|unarchive
```

### Interaction rules

- **Positional classification**: `PROJECT [+TAG ...]`; a bare `cosw start`
  records unassigned time, matching the domain's exceptional fallback.
- **Auto-create on first use**: unknown project/tag names on `start`,
  `switch`, `add`, and `edit` are created in the same commit and announced
  ("created project X"). Archived names resolve but are rejected with a hint
  to unarchive. Filter arguments (`-p`, `-T`, `--ignore-*`) never create.
- **Span references**: a recency index (`-1` = most recent) or an
  unambiguous UUID prefix; omitted means the active span, else the most
  recent.
- **Timestamps**: ISO 8601 or bare `HH:MM[:SS]` meaning today, interpreted in
  local time and stored as UTC. A bare date as a range/`--to` end means the
  end of that day.
- **Confirmation**: `cancel` and `remove` prompt unless `--force`; aborting
  never commits.
- **Conflicts**: a stale-version commit is reported as an error; the local
  mutation is not silently retried.
- **Data file**: `$XDG_DATA_HOME/context-switch/data.json` by default,
  overridable via `COSW_DATA_FILE` or `--data-file`.

### Core mutations added for the CLI

- `Document::add_span(started_at, stopped_at, project_id, tag_ids, at)` —
  records completed time without requiring an idle timer.
- `Document::remove_span(span_id)` — deletes a span; removing the active one
  discards the timer and clears `active_span_id`.

### Deferred

- Config file and `cosw config` — landed in ADR-0006 —
  offline action queue, colors, `$EDITOR`-based edit,
  and `cosw resume` refusing to run while a timer is active is already
  enforced — a future `resume --switch` escape hatch is possible but not
  planned.

## Consequences

- Good, because the surface is immediately familiar to a Watson user while
  speaking the project's domain language.
- Good, because auto-create keeps the common path to one command without
  weakening referential integrity.
- Bad, because positional `+TAG` parsing means project names cannot start
  with `+`, and `-N` indexing requires `ignore_unknown_options` which also
  swallows misspelled flags on `edit`/`remove`/`resume`.
- Neutral, because report/log filtering semantics (AND for tags, OR for
  projects, duration clipped to the range) follow Watson's model.

## Implementation Plan

- **Affected paths**: `cli/cosw/{cli,core,timeparse,reporting,history,manage}.py`,
  `cli/tests/*`, `libs/contextswitch-core/src/domain.rs` (+ stubs).
- **Patterns to follow**: commands are thin — `transact` reads a snapshot,
  applies one mutation, commits against the observed version; name and span
  resolution live in `cosw.core`; all timestamps come from `timeparse`.
- **Patterns to avoid**: mutating the document outside `transact`; creating
  projects/tags from filter arguments; any second path that stops the active
  span.

## Verification

- [x] `cosw start`/`switch`/`resume` enforce the one-active-timer rule
  through the commit boundary.
- [x] `cosw cancel` and `cosw remove` discard spans without touching
  unrelated records (core tests).
- [x] `--json` output is emitted by every read command.
- [x] `cosw add` works while a timer is active (core `add_span`).
- [x] 95% coverage gate holds (`task check`).
