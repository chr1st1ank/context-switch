# context-switch

A single-user, multi-device time tracking system with synchronized storage.

[![CI](https://github.com/chr1st1ank/context-switch/actions/workflows/ci.yml/badge.svg)](https://github.com/chr1st1ank/context-switch/actions/workflows/ci.yml)
[![CodeQL](https://github.com/chr1st1ank/context-switch/actions/workflows/codeql-analysis.yml/badge.svg)](https://github.com/chr1st1ank/context-switch/actions/workflows/codeql-analysis.yml)
[![Python 3.14+](https://img.shields.io/badge/python-3.14+-blue.svg)](https://www.python.org/downloads/)

## Overview

**context-switch** provides time tracking across multiple devices with a synchronized data store. The user can:

- Start, stop, and switch timers
- Organize time by projects and tags
- Generate reports from raw time data
- Work offline with automatic synchronization when connectivity returns

### Components

- **cosw**: Command-line client for laptops (Python/Click)
- **Android**: Native Android application
- **Dashboard**: Reporting and visualization (planned)
- **contextswitch-core**: Shared domain model and storage provider interface (Rust with Python bindings)

## Quick start

### Installation

```bash
git clone https://github.com/chr1st1ank/context-switch.git
cd context-switch
uv sync
```

### Running the CLI

```bash
uv run cosw --help
uv run cosw start --project "My Project"
uv run cosw stop
uv run cosw switch --project "Another Project"
```

### Running tests

```bash
task check    # lint + typecheck + test
task test     # just tests
```

## Architecture

The system is built around a **storage provider abstraction** that supports both local filesystem and remote object storage:

```text
cosw ──────────┐
Android ───────┼── client domain/sync/report core
Dashboard ────┘
                  │
          storage-provider interface
                  │
      ┌───────────┴───────────┐
      │                       │
  local filesystem    remote object storage
```

Key principles:

1. **Central consistency**: The storage provider is authoritative; clients cache data but don't silently overwrite newer data
2. **Short transactions**: Read a version, make one small mutation, conditionally commit
3. **One global active timer**: Enforced by the storage transaction
4. **Portable contracts**: Versioned JSON schemas are the cross-language source of truth
5. **Reports are derived**: Computed on demand from raw synchronized records
6. **Provider independence**: Local and remote storage implement the same interface

See `docs/architecture.md` for the full design.

## Domain language

Key terms:

- **Span**: A mutable record of a period during which the user records time (start, optional stop, optional project, tags)
- **Active timer**: The one span whose stop time has not been recorded
- **Switch**: Stop the active timer and start a new one in one logical action
- **Project**: A named work context to which time can be assigned
- **Tag**: A reusable label that can be attached to a span
- **Storage provider**: An implementation of the interface through which clients read and write synchronized data

See `CONTEXT.md` for the complete domain language.

## Development

### Prerequisites

- Python 3.14+
- Rust (stable, installed via `rustup`)
- `uv` (installed via `mise`)
- `task` (installed via `mise`)

### Common tasks

```bash
task check          # lint + typecheck + test (pre-push gate)
task lint           # ruff check --fix + ruff format
task typecheck      # ty check
task test           # pytest with coverage
task changelog      # preview unreleased notes
```

### Project structure

```text
context-switch/
├── cli/                    # cosw CLI client
├── android/                # Android client
├── libs/
│   └── contextswitch-core/ # Shared domain model and storage interface
├── docs/
│   ├── architecture.md     # System design
│   ├── glossary.md         # Domain language
│   └── decisions/          # Architecture Decision Records
├── CONTEXT.md              # Domain language reference
└── AGENTS.md               # Agent guidance for development
```

### Code style

- **Python**: Python 3.14, `ruff` for lint/format, `ty` for type-checking
- **Rust**: Stable, `cargo fmt` for formatting, `cargo clippy` for linting
- **Testing**: `pytest` for Python, `cargo test` for Rust; 95% coverage gate for libraries
- **Commits**: Conventional Commits for semantic versioning

### Testing with sample data

Use `scripts/simulate.py` to generate realistic working time data for manual testing and demos:

```bash
# Generate 14 days of sample data (weekdays only, 8h days)
COSW_DATA_FILE=/tmp/test.json ./scripts/simulate.py 14

# View the generated data
COSW_DATA_FILE=/tmp/test.json cosw report --range all
COSW_DATA_FILE=/tmp/test.json cosw log --range week
```

The script generates:

- Weekdays only; 8h workdays starting 08:00–09:30
- 50% coding (45–135m blocks), 25% meetings (20–70m), 25% orga (30–90m)
- Realistic breaks: one lunch break (30–60m) and ~15% short gaps (10–30m)
- Half of all breaks recorded as unassigned spans; the rest left as gaps
- Today's blocks clamped to now (never records into the future)

## Offline behavior

The system is online-first. When disconnected:

- Clients provide degraded read-only access to cached data
- Timer lifecycle actions (start, stop, switch) are queued locally
- Other mutations require connectivity
- Queued actions sync immediately when connectivity returns

## Synchronization and conflicts

The normal mutation flow is:

1. Read canonical data and its version
2. Apply one focused mutation in memory
3. Commit conditionally against that version
4. On success, update the client cache
5. On stale-version failure, preserve the local mutation as conflict data

The initial design does not provide interactive merge resolution. Conflicted offline actions remain available for export or copying into a message.

## Security

- Credentials are stored through the platform's secure credential facility, not in the synchronized JSON logbook
- Encryption is a storage-provider configuration option
- The provider boundary permits an encryption wrapper for future use

## License

Licensed under the Apache License, Version 2.0 — see [LICENSE](LICENSE) for details.

## Documentation

- [System Architecture](docs/architecture.md)
- [Implementation Backlog](docs/backlog.md)
- [Domain Language](CONTEXT.md)
- [Architecture Decisions](docs/decisions/)
- [Agent Guidance](AGENTS.md)

## Contributing

See `AGENTS.md` for development guidelines.
