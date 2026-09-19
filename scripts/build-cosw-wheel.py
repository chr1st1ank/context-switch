#!/usr/bin/env python3
"""Build the cosw wheel with contextswitch-core baked in.

contextswitch-core is an internal, unpublished library, so it cannot be a
regular wheel dependency of cosw (see docs/decisions for context). Instead
this script builds each package independently with its native tool, then
merges the compiled `contextswitch_core` package into the `cosw` wheel using
the standard `wheel` CLI (https://pypi.org/project/wheel/):

1. `maturin build` compiles contextswitch-core and produces a correctly
   tagged wheel (e.g. `cp310-abi3-manylinux_2_38_x86_64`).
2. `uv build` produces a plain, pure-Python `cosw` wheel and sdist.
3. `wheel unpack` both wheels, copy the compiled `contextswitch_core/`
   package into the unpacked cosw tree, flip `Root-Is-Purelib` to false,
   `wheel pack` (which recomputes RECORD hashes) and `wheel tags` to rename
   the result to contextswitch-core's platform/ABI tag.

The sdist is copied through unmodified: installing from source requires a
Rust toolchain and is not a goal here, only prebuilt-wheel installs are.
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CORE_DIR = REPO_ROOT / "libs" / "contextswitch-core"
CLI_DIR = REPO_ROOT / "cli"


def run(cmd: list[str], **kwargs) -> None:
    print(f"+ {' '.join(cmd)}", file=sys.stderr)
    subprocess.run(cmd, check=True, **kwargs)


def build_core_wheel(stage: Path) -> Path:
    run(["uv", "run", "maturin", "build", "--release", "--out", str(stage)], cwd=CORE_DIR)
    (wheel,) = stage.glob("contextswitch_core-*.whl")
    return wheel


def build_cosw_dist(stage: Path) -> tuple[Path, Path]:
    run(["uv", "build", "--package", "cosw", "--out-dir", str(stage)], cwd=REPO_ROOT)
    (wheel,) = stage.glob("cosw-*.whl")
    (sdist,) = stage.glob("cosw-*.tar.gz")
    return wheel, sdist


def wheel_tag(wheel_path: Path) -> tuple[str, str, str]:
    """The (python, abi, platform) tag triple encoded in a wheel filename."""
    python_tag, abi_tag, platform_tag = wheel_path.stem.split("-")[-3:]
    return python_tag, abi_tag, platform_tag


def merge_wheels(core_wheel: Path, cosw_wheel: Path, unpack_dir: Path) -> Path:
    run([sys.executable, "-m", "wheel", "unpack", str(core_wheel), "--dest", str(unpack_dir)])
    run([sys.executable, "-m", "wheel", "unpack", str(cosw_wheel), "--dest", str(unpack_dir)])

    (core_extracted,) = unpack_dir.glob("contextswitch_core-*")
    (cosw_extracted,) = unpack_dir.glob("cosw-*")

    shutil.copytree(
        core_extracted / "contextswitch_core",
        cosw_extracted / "contextswitch_core",
        dirs_exist_ok=True,
    )

    (wheel_metadata,) = cosw_extracted.glob("*.dist-info/WHEEL")
    wheel_metadata.write_text(
        re.sub(
            r"^Root-Is-Purelib: true$",
            "Root-Is-Purelib: false",
            wheel_metadata.read_text(),
            flags=re.MULTILINE,
        )
    )

    run([sys.executable, "-m", "wheel", "pack", str(cosw_extracted), "--dest-dir", str(unpack_dir)])
    (repacked,) = unpack_dir.glob("cosw-*-py3-none-any.whl")

    python_tag, abi_tag, platform_tag = wheel_tag(core_wheel)
    run(
        [
            sys.executable,
            "-m",
            "wheel",
            "tags",
            "--remove",
            f"--python-tag={python_tag}",
            f"--abi-tag={abi_tag}",
            f"--platform-tag={platform_tag}",
            str(repacked),
        ]
    )
    (final_wheel,) = unpack_dir.glob(f"cosw-*-{python_tag}-{abi_tag}-{platform_tag}.whl")
    return final_wheel


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--out-dir",
        type=Path,
        default=REPO_ROOT / "dist",
        help="Where to place the final dist files",
    )
    args = parser.parse_args()
    args.out_dir.mkdir(parents=True, exist_ok=True)

    with tempfile.TemporaryDirectory(prefix="cosw-build-") as tmp:
        stage = Path(tmp)
        core_wheel = build_core_wheel(stage / "core")
        cosw_wheel, cosw_sdist = build_cosw_dist(stage / "cosw")
        final_wheel = merge_wheels(core_wheel, cosw_wheel, stage / "merged")

        shutil.copy2(final_wheel, args.out_dir)
        shutil.copy2(cosw_sdist, args.out_dir)

    print(f"Built {args.out_dir / final_wheel.name}", file=sys.stderr)
    print(f"Built {args.out_dir / cosw_sdist.name}", file=sys.stderr)


if __name__ == "__main__":
    main()
