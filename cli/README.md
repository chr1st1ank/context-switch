# cosw — context-switch CLI

The command-line client for context-switch time tracking.

## Installation

```bash
uv sync
uv run cosw --help
```

## Development

```bash
task lint      # format and lint
task typecheck # type-check
task test      # run tests
task check     # all of the above
```

## Architecture

The CLI integrates with the storage provider interface to read and write time tracking data. It provides:

- Timer lifecycle: start, stop, switch
- Project and tag management
- Report generation
- Offline queueing of timer actions

See `docs/architecture.md` for the full system design.
