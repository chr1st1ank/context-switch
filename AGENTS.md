# Agent Instructions: context-switch

This is a multi-component time-tracking system with synchronized storage across multiple devices. Agents working here should follow these conventions to maintain consistency.

## Project overview

**context-switch** is a single-user, multi-device time tracking system with:

- **cosw**: Command-line client for laptops (Python/Click); published on
  PyPI as the `contextswitch` distribution
- **Android**: Native Android application
- **Dashboard**: Reporting and visualization (future)
- **contextswitch-core**: Shared domain model and storage provider interface (Rust with Python bindings)

The system is designed around a storage provider abstraction that supports both local filesystem and remote object storage, with conditional writes to prevent conflicts. The core library is implemented in Rust for performance and safety, with Python bindings via PyO3 for use by the CLI and other Python components.

See `CONTEXT.md` for the domain language and `docs/architecture.md` for the full system design.

## Repository structure

```text
context-switch/
├── cli/                          # cosw CLI client
│   ├── cosw/                     # package
│   ├── tests/
│   ├── pyproject.toml
│   └── Taskfile.yml
├── android/                      # Android client (Kotlin/Compose over the Rust core)
│   ├── app/                      # Gradle app module (UI, TimerService, settings)
│   ├── build-rust.sh             # cargo-ndk + UniFFI Kotlin binding generation
│   └── Taskfile.yml
├── libs/
│   ├── contextswitch-core/       # Shared domain model and storage interface (Rust)
│   │   ├── src/
│   │   │   ├── lib.rs            # PyO3 module entry point (`python` feature)
│   │   │   ├── domain.rs         # Span, Project, Tag, Logbook types
│   │   │   ├── storage.rs        # StorageProvider interface + LocalFsProvider
│   │   │   ├── provider.rs       # Generic StorageProvider over a BlobStore + Cipher
│   │   │   ├── blob.rs           # BlobStore layer + LocalFs/S3 stores
│   │   │   ├── s3.rs             # S3BlobStore + SigV4 signing
│   │   │   ├── crypto.rs         # Client-side envelope encryption (age)
│   │   │   ├── portable_config.rs # cosw config.toml parse/serialize for settings transfer
│   │   │   ├── sync.rs           # SyncEngine replica + MemoryCachingProvider decorator
│   │   │   └── conformance.rs    # Provider conformance test suite
│   │   ├── python/               # Python type stubs
│   │   ├── tests/                # Rust integration tests + Python binding tests
│   │   ├── Cargo.toml            # Rust dependencies (`python` feature gates PyO3)
│   │   ├── pyproject.toml        # Python build config (maturin)
│   │   └── Taskfile.yml
│   └── contextswitch-uniffi/     # UniFFI bindings for the Android client (ADR-0009)
├── docs/
│   ├── architecture.md           # System design
│   ├── envelope-format.md        # Encrypted logbook envelope format
│   ├── packaging.md              # How the contextswitch wheel bundles contextswitch-core
│   ├── diagrams/                 # Excalidraw sources + exports
│   └── adr/                      # Architecture Decision Records
├── scripts/
│   ├── build-cosw-wheel.py       # Merges contextswitch-core into the contextswitch wheel
│   └── simulate.py               # Generates realistic sample data via `cosw add`
├── .github/
│   ├── dependabot.yml            # uv, cargo, gradle, actions; 7-day cooldown
│   └── workflows/                # ci, release, codeql-analysis, pr-title
├── CONTEXT.md                    # Domain glossary (domain-modeling format)
├── AGENTS.md                     # This file
├── README.md                     # Project overview
├── SECURITY.md                   # Vulnerability reporting
├── Taskfile.yml                  # Root task runner
├── pyproject.toml                # uv workspace root
├── Cargo.toml                    # Cargo workspace root (single Cargo.lock)
├── cliff.toml                    # git-cliff release notes config
├── mise.toml                     # Tool versions
├── rust-toolchain.toml           # Rust toolchain pin
└── .pre-commit-config.yaml       # Git hooks, run by prek
```

## Dependency direction

- **cli** and **android** both depend on **contextswitch-core**
- Components do not import each other; they communicate through the storage provider interface
- External service access (storage providers, APIs) sits behind small interfaces for testability

## Build and test commands

All operations run through `task`:

```bash
task check               # lint + typecheck + test (the pre-push gate)
task lint                # ruff check --fix + ruff format + lint-rust
task lint-rust           # cargo fmt --check + clippy over the Cargo workspace
task typecheck           # ty check
task test                # test-python + test-rust
task test-python         # build core bindings (maturin develop) + pytest with coverage
task test-rust           # cargo test over the Cargo workspace
task prek                # run all git hooks against all files (CI's lint job)
task update-dependencies # upgrade uv.lock, Cargo.lock and hook revisions, then check
task changelog           # preview unreleased notes
task draft-release       # tag + draft GitHub release (humans only)
```

Tool setup: `mise install` provides python, uv, task, git-cliff, prek and
Rust; then `prek install` once per clone. `uv` must be ≥ 0.9.17 — older
versions cannot parse the relative `exclude-newer = "7 days"` cooldown and
silently drop it from `uv.lock`.

Lockfiles are locked: CI runs with `UV_LOCKED=1` and cargo `--locked`.
They change only through `task update-dependencies`, a Dependabot PR, or a
deliberate `uv add` / `cargo add`. uv enforces a 7-day release cooldown
(`[tool.uv] exclude-newer`), prek does the same for hooks; stable cargo has
no cooldown yet.

Component-specific tasks:

```bash
task cli:dev      # run CLI in development mode
task cli:test     # run CLI tests
task cli:build    # build the contextswitch wheel with contextswitch-core baked in
task core:test    # run core library tests
```

See `docs/packaging.md` for how `task cli:build` bundles the unpublished
`contextswitch-core` library into the `contextswitch` wheel.

Releases are tag-driven: versions are never committed to manifests, which
keep a static placeholder. `release.yml` stamps the release version into
the wheel at build time, and the Android job derives
`versionName`/`versionCode` from the tag. `task draft-release` creates the
tag and a draft GitHub release; publishing the draft triggers
`release.yml`.

## Python environment

- **Python version**: 3.14 (pinned in `.python-version`)
- **Package manager**: `uv` with workspace at the root
- **Single lockfile**: `uv.lock` at the root covers all Python components
- **Lint/format**: `ruff check` and `ruff format`
- **Type checking**: `ty` with all rules at error severity
- **Testing**: `pytest` with coverage gate at 95%

Setup:

```bash
uv sync                 # install all dependencies
uv sync --package contextswitch  # install only CLI dependencies
```

## Rust environment

The `contextswitch-core` library is implemented in Rust:

- **Rust version**: Stable (pinned in `rust-toolchain.toml`)
- **Build system**: Cargo with maturin for Python bindings
- **Python bindings**: PyO3 for seamless Python interop
- **Serialization**: Serde for JSON and other formats
- **Type safety**: Leverages Rust's type system for correctness

Build and test:

```bash
cd libs/contextswitch-core
task build      # build the Rust library
task lint       # check with clippy
task test       # run Rust tests
task fmt        # format code
```

The compiled extension is used by Python components via `import contextswitch_core`.

## Code style and patterns

### Python

- Flat package layout (no `src/` directory)
- Dataclasses and `Protocol`s for domain types
- External services behind small interfaces
- Tests co-located in `tests/` at the component root
- Type hints required; `ty` enforces them

### Rust

- Standard Cargo layout with `src/` directory
- Modules: `domain.rs`, `storage.rs`, `conformance.rs`, `lib.rs`
- PyO3 wrappers for Python types (e.g., `PySpan`, `PyProject`, `PyTag`)
- Error handling with custom `StorageError` enum
- Comprehensive documentation comments

### Domain model

The core domain types are implemented in Rust (`libs/contextswitch-core/src/domain.rs`) and exposed to Python via PyO3:

- **Span**: A mutable record of time with start, optional stop, optional project, and tags
  - Rust: `struct Span` with `is_active()` method
  - Python: `PySpan` class with properties and methods
- **Project**: A named work context with stable identity
  - Rust: `struct Project`
  - Python: `PyProject` class
- **Tag**: A reusable label for spans
  - Rust: `struct Tag`
  - Python: `PyTag` class
- **Active timer**: The one span with `stopped_at = null`; enforced by storage transaction

All types use:

- UUID identifiers (stable across versions)
- ISO 8601 timestamps (UTC)
- Serde for JSON serialization

See `CONTEXT.md` for the full domain language.

### Storage provider interface

The `StorageProvider` trait is defined in Rust (`libs/contextswitch-core/src/storage.rs`):

```rust
pub trait StorageProvider: Send + Sync {
    fn read(&self) -> Result<StorageSnapshot, StorageError>;
    fn commit(&self, logbook: Logbook, expected_version: &str) -> Result<String, StorageError>;
}
```

`commit` returns the new version on success and fails with
`StorageError::Conflict` on a stale `expected_version`. Every provider must
pass the conformance suite in `libs/contextswitch-core/src/conformance.rs`.
See ADR-0004 for the logbook schema and the local commit protocol.

Implementations:

- Local filesystem (initial)
- Remote object storage (future)

Clients handle conflicts by preserving local mutations and reporting them; interactive merge resolution is out of scope initially.

## Architecture Decision Records

ADRs live in `docs/adr/` with MADR-style templates:

- Numeric filenames: `0001-short-title.md`
- Status in front matter: proposed, accepted, superseded, deprecated
- Indexed in `docs/adr/README.md`

Before implementing significant changes, check existing ADRs and consider whether a new one is needed.

## CI/CD

Five workflows in `.github/workflows/`:

- **ci.yml**: prek hooks, type-check, Python tests with coverage gate, Rust
  workspace lint/test, Android lint/test/debug APK
- **release.yml**: on a published release, stamps the tag version, builds
  the `contextswitch` wheel and the signed Android APK, and attaches both
  to the GitHub release (PyPI publishing is not enabled yet)
- **test-release.yml**: manual dispatch; stamps a `0.0.1-devN` version
  computed from the newest release on the index and publishes the wheel to
  TestPyPI via OIDC trusted publishing (GitHub environment `test-pypi`)
- **codeql-analysis.yml**: CodeQL scanning for python, actions and rust
- **pr-title.yml**: Conventional Commits check on the PR title — PRs are
  squash-merged, so the title becomes the commit git-cliff versions from

Dependabot opens weekly `chore(deps):` PRs for uv, cargo, gradle and
actions with a 7-day cooldown. Hook revisions are only bumped by
`task update-dependencies`.

## Testing strategy

- **Unit tests**: Test domain logic and storage interface implementations
- **Integration tests**: Test component interactions through the storage provider
- **Fixtures**: Shared in `conftest.py` per component
- **Coverage gate**: 95% over `cli` and `libs` combined (deliberately stricter than the standard's library-only gate)

Write tests first when fixing bugs or adding features (TDD preferred).

## Offline behavior and synchronization

The system is online-first. When disconnected:

- Clients provide degraded read-only access to cached data
- Timer lifecycle actions (start, stop, switch) are queued locally
- Other mutations require connectivity
- Queued actions sync immediately when connectivity returns
- Conflicts are preserved as local mutations; no automatic merge

See `docs/architecture.md` section 7 for details.

## Implementation backlog

Outstanding work items are tracked in [GitHub Issues](https://github.com/chr1st1ank/context-switch/issues).
Check open issues before planning new features (`gh issue list`).

## When in doubt

- Check `docs/architecture.md` for system design

- Check [GitHub Issues](https://github.com/chr1st1ank/context-switch/issues) for outstanding work items
- Check `docs/packaging.md` for how the contextswitch wheel is built
- Check `CONTEXT.md` for domain language
- Check existing ADRs in `docs/adr/`
- Run `task check` before pushing
- Follow the patterns in existing code

## Do not

- Run `task draft-release` — it pushes tags and creates GitHub releases
- Commit secrets, credentials, or environment-specific values
- Modify CI security policies or compliance controls to work around failures
- Add dependencies without checking that they're stable (prefer versions published >7 days ago)
- Break the storage provider abstraction; all client-specific logic stays in the client
