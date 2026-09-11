# cosw — context-switch CLI

The command-line client for context-switch time tracking.

## Installation

```bash
uv sync
uv run cosw --help
```

## Usage

```bash
cosw start apollo11 +module +brakes   # start a timer (auto-creates names)
cosw status                           # show the active timer
cosw switch personal                  # stop current, start new — one action
cosw stop
cosw resume                           # restart the last span's classification
cosw cancel --force                   # discard the active timer

cosw add apollo11 --from 08:00 --to 09:30   # record time not tracked live
cosw edit -1 +focus                         # tag the most recent span
cosw remove -1 --force                      # delete a span

cosw log --week                      # spans this week
cosw report --by project --json      # totals, machine-readable
cosw report --by day --from 2026-09-01 --to 2026-09-11

cosw projects                        # list; also: add|rename|archive|unarchive
cosw tags --all                      # include archived
```

Spans are addressed by recency index (`-1` = most recent) or ID prefix.
Timestamps accept ISO 8601 or `HH:MM` (today, local time).

## Data file

Canonical data lives at `$XDG_DATA_HOME/context-switch/data.json` by default.
Override with `COSW_DATA_FILE` or `cosw --data-file PATH ...`.

## Configuration

An optional TOML config file lives at
`$XDG_CONFIG_HOME/context-switch/config.toml` (override with `COSW_CONFIG`
or `cosw --config PATH`). Run `cosw config` to open it in `$EDITOR` — a
commented skeleton is created on first use — or `cosw config --path` to
print the resolved path.

```toml
[storage]
provider = "local"                          # only "local" for now
data_file = "~/canonical/data.json"         # ~ and $VARS expanded;
                                            # relative resolves vs this dir
```

Precedence: `--data-file` flag > `COSW_DATA_FILE` env > `storage.data_file`
> default. `cosw status -v` shows which source won and which config file
was read. Config holds non-secret settings only — provider credentials
belong to environment variables or the platform credential facility.

## Development

```bash
task lint      # format and lint
task typecheck # type-check
task test      # run tests
task check     # all of the above
```

## Architecture

The CLI talks to canonical data exclusively through the storage-provider
interface (`contextswitch-core`): every command is one read–mutate–commit
transaction. The pending-offline-action queue is deferred until a remote
provider exists.

See `docs/architecture.md` for the system design and `docs/decisions/` for
ADRs, including the CLI command surface (ADR-0005).
