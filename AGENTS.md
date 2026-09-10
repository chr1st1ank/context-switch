# Agent Instructions: context-switch

This is a multi-component time-tracking system with synchronized storage across multiple devices. Agents working here should follow these conventions to maintain consistency.

## Project overview

**context-switch** is a single-user, multi-device time tracking system with:

- **cosw**: Command-line client for laptops (Python/Click)
- **Android**: Native Android application
- **Dashboard**: Reporting and visualization (future)
- **contextswitch-core**: Shared domain model and storage provider interface

The system is designed around a storage provider abstraction that supports both local filesystem and remote object storage, with conditional writes to prevent conflicts.

See `CONTEXT.md` for the domain language and `docs/architecture.md` for the full system design.

## Repository structure

```text
context-switch/
├── cli/                          # cosw CLI client
│   ├── cosw/                     # package
│   ├── tests/
│   ├── pyproject.toml
│   └── Taskfile.yml
├── android/                      # Android client
│   ├── README.md
│   └── Taskfile.yml
├── libs/
│   └── contextswitch-core/       # Shared domain model and storage interface
│       ├── contextswitch_core/
│       ├── tests/
│       ├── pyproject.toml
│       └── Taskfile.yml
├── docs/
│   ├── architecture.md           # System design
│   ├── design.md                 # Implementation details (TBD)
│   ├── glossary.md               # Domain language
│   └── decisions/                # Architecture Decision Records
├── CONTEXT.md                    # Domain language reference
├── AGENTS.md                     # This file
├── README.md                     # Project overview
├── Taskfile.yml                  # Root task runner
├── pyproject.toml                # uv workspace root
├── mise.toml                     # Tool versions
└── .pre-commit-config.yaml       # Pre-commit hooks
```

## Dependency direction

- **cli** and **android** both depend on **contextswitch-core**
- Components do not import each other; they communicate through the storage provider interface
- External service access (storage providers, APIs) sits behind small interfaces for testability

## Build and test commands

All operations run through `task`:

```bash
task check        # lint + typecheck + test (the pre-push gate)
task lint         # ruff check --fix + ruff format
task typecheck    # ty check
task test         # pytest with coverage
task changelog    # preview unreleased notes
```

Component-specific tasks:

```bash
task cli:dev      # run CLI in development mode
task cli:test     # run CLI tests
task core:test    # run core library tests
```

## Python environment

- **Python version**: 3.14 (pinned in `.python-version`)
- **Package manager**: `uv` with workspace at the root
- **Single lockfile**: `uv.lock` at the root covers all Python components
- **Lint/format**: `ruff check` and `ruff format`
- **Type checking**: `ty` with all rules at error severity
- **Testing**: `pytest` with coverage gate at 95%

Setup:

```bash
uv sync              # install all dependencies
uv sync --package cli  # install only CLI dependencies
```

## Code style and patterns

### Python

- Flat package layout (no `src/` directory)
- Dataclasses and `Protocol`s for domain types
- External services behind small interfaces
- Tests co-located in `tests/` at the component root
- Type hints required; `ty` enforces them

### Domain model

The core domain types are in `contextswitch_core.domain`:

- **Span**: A mutable record of time with start, optional stop, optional project, and tags
- **Project**: A named work context with stable identity
- **Tag**: A reusable label for spans
- **Active timer**: The one span with `stopped_at = null`; enforced by storage transaction

See `CONTEXT.md` for the full domain language.

### Storage provider interface

All clients use the `StorageProvider` interface from `contextswitch_core.storage`:

- `read()`: Get current canonical data and version
- `commit(snapshot, expected_version)`: Conditionally write against observed version

Implementations:

- Local filesystem (initial)
- Remote object storage (future)

Clients handle conflicts by preserving local mutations and reporting them; interactive merge resolution is out of scope initially.

## Architecture Decision Records

ADRs live in `docs/decisions/` with MADR-style templates:

- Numeric filenames: `0001-short-title.md`
- Status in front matter: proposed, accepted, superseded, deprecated
- Indexed in `docs/decisions/README.md`

Before implementing significant changes, check existing ADRs and consider whether a new one is needed.

## CI/CD

Three mandatory workflows in `.github/workflows/`:

- **ci.yml**: Pre-commit, type-check, tests with coverage gate
- **release.yml**: Publish to PyPI on release
- **codeql-analysis.yml**: CodeQL security scanning

Dependabot is configured for weekly dependency updates.

## Testing strategy

- **Unit tests**: Test domain logic and storage interface implementations
- **Integration tests**: Test component interactions through the storage provider
- **Fixtures**: Shared in `conftest.py` per component
- **Coverage gate**: 95% for libraries, no gate for apps/services

Write tests first when fixing bugs or adding features (TDD preferred).

## Offline behavior and synchronization

The system is online-first. When disconnected:

- Clients provide degraded read-only access to cached data
- Timer lifecycle actions (start, stop, switch) are queued locally
- Other mutations require connectivity
- Queued actions sync immediately when connectivity returns
- Conflicts are preserved as local mutations; no automatic merge

See `docs/architecture.md` section 7 for details.

## When in doubt

- Check `docs/architecture.md` for system design
- Check `CONTEXT.md` for domain language
- Check existing ADRs in `docs/decisions/`
- Run `task check` before pushing
- Follow the patterns in existing code

## Do not

- Commit secrets, credentials, or environment-specific values
- Modify CI security policies or compliance controls to work around failures
- Add dependencies without checking that they're stable (prefer versions published >7 days ago)
- Break the storage provider abstraction; all client-specific logic stays in the client
