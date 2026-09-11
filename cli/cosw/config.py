"""Client configuration file: location, loading, and validation.

The config file is TOML at ``$XDG_CONFIG_HOME/context-switch/config.toml``.
It holds client-local, non-secret settings only; per ADR-0006 secrets never
live here. A missing default file means defaults; an explicitly requested
path (``--config``/``COSW_CONFIG``) must exist.
"""

from __future__ import annotations

import os
import tomllib
from dataclasses import dataclass
from pathlib import Path

import click

ENV_CONFIG = "COSW_CONFIG"

_KNOWN_TOP_LEVEL = {"storage"}
_KNOWN_STORAGE_KEYS = {"provider", "data_file"}
_KNOWN_PROVIDERS = {"local"}

CONFIG_SKELETON = """\
# cosw configuration — context-switch CLI
# Precedence: --data-file flag > COSW_DATA_FILE env > this file > default.

[storage]
# Storage provider. Only "local" is currently supported.
provider = "local"

# Canonical data file. ~ and $VARS are expanded; relative paths resolve
# against this file's directory.
# data_file = "~/.local/share/context-switch/data.json"
"""


@dataclass
class Config:
    """Validated contents of the client config file."""

    path: Path
    provider: str = "local"
    data_file: Path | None = None


def default_config_file() -> Path:
    """The default config file under the XDG config home."""
    xdg = os.environ.get("XDG_CONFIG_HOME")
    base = Path(xdg).expanduser() if xdg else Path.home() / ".config"
    return base / "context-switch" / "config.toml"


def _warn_unknown(path: Path, key: str) -> None:
    click.echo(f"warning: {path}: ignoring unknown key {key!r}", err=True)


def _expand(value: str, config_dir: Path) -> Path:
    path = Path(os.path.expandvars(os.path.expanduser(value)))
    if not path.is_absolute():
        path = config_dir / path
    return path.resolve()


def load_config(path: Path, explicit: bool) -> Config:
    """Load and validate the config file at ``path``.

    ``explicit`` is true when the path came from ``--config`` or
    ``COSW_CONFIG``; an explicit path that does not exist is an error,
    while a missing default file just yields defaults.
    """
    if not path.exists():
        if explicit:
            raise click.ClickException(f"config file not found: {path}")
        return Config(path=path)
    try:
        raw = tomllib.loads(path.read_text())
    except tomllib.TOMLDecodeError as e:
        raise click.ClickException(f"invalid config file {path}: {e}") from e

    config = Config(path=path)
    for key in raw:
        if key not in _KNOWN_TOP_LEVEL:
            _warn_unknown(path, key)
    storage = raw.get("storage", {})
    if not isinstance(storage, dict):
        raise click.ClickException(f"{path}: 'storage' must be a table")
    for key in storage:
        if key not in _KNOWN_STORAGE_KEYS:
            _warn_unknown(path, f"storage.{key}")

    provider = storage.get("provider", "local")
    if not isinstance(provider, str):
        raise click.ClickException(f"{path}: 'storage.provider' must be a string")
    if provider not in _KNOWN_PROVIDERS:
        raise click.ClickException(
            f"{path}: unsupported storage provider {provider!r} (only 'local' is available)"
        )
    config.provider = provider

    data_file = storage.get("data_file")
    if data_file is not None:
        if not isinstance(data_file, str) or not data_file:
            raise click.ClickException(f"{path}: 'storage.data_file' must be a non-empty string")
        config.data_file = _expand(data_file, path.parent)
    return config
