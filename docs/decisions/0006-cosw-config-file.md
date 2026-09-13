---
status: "proposed"
date: 2026-09-11
decision-makers: "Project owner"
consulted: ""
informed: ""
---

# cosw client config file

## Context and Problem Statement

ADR-0005 deferred client configuration until it was needed. `cosw`
currently resolves its data file from `--data-file`, `COSW_DATA_FILE`, or
the XDG default — fine for one knob, but the remote object-storage provider
will bring a cluster of settings (provider selection, bucket, region,
credentials references) that do not belong on the command line. A config
file is needed now so that provider settings have a designed home instead
of accreting ad hoc.

## Decision Drivers

- Client-local only: config must never leak into the synchronized logbook.
- Hand-editable: a single user maintains this file; comments and a flat
  shape matter more than machine round-tripping.
- Zero new dependencies where the standard library suffices.
- Forward-compatible: a newer config must not break an older binary.
- Consistent with the existing flag/env/XDG conventions.

## Considered Options

- TOML at `$XDG_CONFIG_HOME/context-switch/config.toml` with a `[storage]`
  table.
- INI (Watson-compatible, `configparser`) — flat sections only.
- JSON — stdlib read and write, but no comments.
- YAML — expressive but a new dependency.

## Decision Outcome

Chosen option: "TOML with a `[storage]` table".

### Location and overrides

- Default path: `$XDG_CONFIG_HOME/context-switch/config.toml`.
- Overridable via `--config` flag or `COSW_CONFIG` env var, mirroring
  `--data-file`/`COSW_DATA_FILE`.
- A missing *default* file means defaults, silently. An *explicitly
  requested* path that does not exist is an error — an explicit path is a
  statement of intent, and a typo must not silently redirect canonical
  data. `COSW_CONFIG=/dev/null` disables config cleanly.
- Exception: `cosw config` never validates the file, because it is the
  tool that creates and repairs it.

### Schema

```toml
[storage]
provider = "local"            # default; other values are errors for now
data_file = "~/data.json"     # ~ and $VARS expanded; relative paths
                              # resolve against the config file's directory
```

`data_file` is scoped under `[storage]` because it is meaningless to a
remote provider. Future remote-provider keys (`provider = "s3"`,
`bucket`, `region`, …) join the same flat table; other setting groups get
their own tables (`[output]`, …).

### Precedence

`--data-file` flag > `COSW_DATA_FILE` env > `storage.data_file` in the
config file > built-in XDG default. `status -v` reports a `storage:` line
(adding `"config file"` as a source) and a `config:` line showing the
resolved path and whether it exists.

### Editing surface

`cosw config` is a group with `invoke_without_command`: bare invocation
opens the file in `$VISUAL`/`$EDITOR` (error if neither is set — no
fallback editor), seeding a commented skeleton on first open.
`cosw config --path` prints the resolved path. The group shape leaves room
for `get`/`set` subcommands later without breaking `cosw config`.

### Strictness and secrets

- Malformed TOML, a non-table `[storage]`, wrong-typed or empty values,
  and unknown `provider` values are hard errors naming file and key.
- Unknown keys produce one stderr warning each and are ignored —
  forward compatibility for free.
- Secrets never live in this file (architecture §10 sends credentials to
  the platform's secure credential facility). When the remote provider
  lands, revisit — likely a `secret_command`/keyring reference, not a
  value.

## Consequences

- Good, because provider settings have a designed home before the remote
  provider exists, and TOML costs nothing to read (`tomllib`).
- Good, because the precedence chain is uniform across flag, env, and file.
- Bad, because two sources can now silently disagree; the `status -v`
  source reporting exists to make that debuggable.
- Neutral, because a future `cosw config set` will need a TOML writer
  (`tomli-w` or constrained emission); deferred until needed.

## Implementation Plan

- **Affected paths**: `cli/cosw/{cli,core,config}.py`, `cli/tests/*`,
  `cli/README.md`.
- **Patterns to follow**: commands stay thin; config loading lives in
  `cosw.config`; the storage provider is opened lazily so `config`,
  `--help`, and `--version` never require a usable data file.
- **Patterns to avoid**: secrets in the config file; auto-writing the
  config outside `cosw config`; client config in the shared Rust core.

## Verification

- [x] Precedence flag > env > config > default, each level tested.
- [x] Missing explicit path errors; missing default is silent.
- [x] `cosw config` works against missing and malformed files.
- [x] Malformed/unknown/ mistyped values behave per the strictness rules.
- [x] 95% coverage gate holds (`task check`).

## Amendment (2026-09-13, see ADR-0007)

The `[storage]` table gains a `provider` selector (`"local"` | `"s3"`) and,
for `"s3"`, `bucket`, `region`, `prefix`, `endpoint`, `use_path_style`,
`profile`, and `passphrase_command`. An unrecognized `provider` value is
rejected with a message naming the valid options, per the original
strictness rules above; unknown keys continue to warn-and-ignore rather
than error, so an older client tolerates a config written by a newer one.
No secret ever belongs in this file — `passphrase_command` names a command
to *run*, never a literal secret — and rejecting an inline secret key
outright is tracked as an interim guard in `docs/backlog.md` pending a
platform secret-store integration.
