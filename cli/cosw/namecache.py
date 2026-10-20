"""Local cache of project and tag names, backing shell completion.

Completion must never touch storage, the network or the passphrase, so it
reads this cache instead. The cache is refreshed by every command that reads
the logbook and holds names in plaintext, scoped per storage location.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
from typing import Literal

import click
from contextswitch_core import Logbook

from cosw.config import Config

Kind = Literal["project", "tag"]

CACHE_ERRORS = (OSError, ValueError, click.ClickException)


def storage_key(config: Config, data_file: Path) -> str:
    """Identify the storage location without opening a provider."""
    if config.provider == "s3":
        return f"s3://{config.bucket}/{config.prefix}?endpoint={config.endpoint or ''}"
    return f"file://{data_file.resolve()}"


def cache_file(key: str) -> Path:
    xdg = os.environ.get("XDG_CACHE_HOME", "")
    base = Path(xdg).expanduser() if xdg else Path.home() / ".cache"
    digest = hashlib.sha256(key.encode()).hexdigest()[:16]
    return base / "context-switch" / f"names-{digest}.json"


def write_names(config: Config, data_file: Path, logbook: Logbook) -> None:
    """Refresh the cache from ``logbook``; failures are ignored."""
    try:
        path = cache_file(storage_key(config, data_file))
        payload = {
            "project": [[p.name, p.archived] for p in logbook.projects()],
            "tag": [[t.name, t.archived] for t in logbook.tags()],
        }
        path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        tmp = path.with_suffix(".tmp")
        fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
        with os.fdopen(fd, "w") as f:
            json.dump(payload, f)
        os.replace(tmp, path)
    except CACHE_ERRORS:
        pass


def read_names(config: Config, data_file: Path, kind: Kind, archived: bool = False) -> list[str]:
    """Cached names of ``kind``, active or archived; empty on any failure."""
    try:
        raw = json.loads(cache_file(storage_key(config, data_file)).read_text())
        names = [name for name, is_archived in raw[kind] if bool(is_archived) == archived]
    except (*CACHE_ERRORS, KeyError, TypeError):
        return []
    return sorted((n for n in names if isinstance(n, str)), key=str.casefold)
