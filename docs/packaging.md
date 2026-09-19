# Packaging cosw

`contextswitch-core` is unpublished, so it can't be a normal wheel dependency
of `cosw` — it's a **dev-only** dependency (`[dependency-groups] dev` in
`cli/pyproject.toml`), installed for local development via `uv sync` but
never listed in the built wheel's `Requires-Dist`.

`task cli:build` runs `scripts/build-cosw-wheel.py`, which builds each
package independently with its native tool and merges them:

1. `maturin build` compiles contextswitch-core into a correctly tagged wheel
   (e.g. `cp310-abi3-manylinux_2_38_x86_64`).
2. `uv build` produces a plain, pure-Python `cosw` wheel and sdist.
3. The standard `wheel` CLI (`unpack` / `pack` / `tags`) merges the compiled
   `contextswitch_core` package into the `cosw` wheel and retags it to match.

No custom build backend or build hook is involved — each tool does what it's
already good at, and the sdist is left untouched. Installing `cosw` from
source (rather than a wheel) will not include the compiled bindings; that's
intentional, since this isn't published anywhere sdists would be preferred.

## Why not a build hook or maturin backend?

Two more "integrated" approaches were tried and rejected:

- **A custom hatchling build hook** that shelled out to `maturin build` and
  force-included the result. It worked, but required a `BuildHookInterface`
  subclass, temp directories, wheel-unzipping, and sdist `force-include`
  entries mirroring the crate layout — a lot of bespoke code to maintain.
- **Maturin as the build backend directly** (with `manifest-path` and
  `python-source` pointing outside the project, plus `python-packages` for
  the `cosw` sources). This actually works when invoked via the `maturin`
  CLI, but breaks when `uv build`/`pip` invoke maturin's PEP 517 hooks
  directly: the manylinux/auditwheel tag detection that the CLI performs is
  skipped, producing an unpublishable bare `linux_x86_64` tag. This is a
  known, unresolved issue upstream (astral-sh/uv#9842).

The current script avoids both problems: `maturin build` is always invoked as
a CLI subprocess (correct tags), and merging is done with the standard
`wheel` tool rather than a bespoke build backend.
