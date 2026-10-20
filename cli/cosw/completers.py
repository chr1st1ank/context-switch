"""Dynamic shell-completion callbacks for project and tag names.

Completion runs without the root command's callback, so the config and data
file are re-resolved here from the root context's parsed parameters. Nothing
below opens a storage provider; names come from the local cache only.
"""

from __future__ import annotations

import contextlib
import io
from collections.abc import Callable
from pathlib import Path

import click
from click.shell_completion import CompletionItem

from cosw.config import Config, default_config_file, load_config
from cosw.core import default_data_file
from cosw.namecache import CACHE_ERRORS, Kind, read_names

Completer = Callable[[click.Context, click.Parameter, str], list[CompletionItem]]


def _cached(ctx: click.Context, kind: Kind, archived: bool) -> list[str]:
    root = ctx.find_root()
    config_file = root.params.get("config_file")
    assert config_file is None or isinstance(config_file, Path)
    try:
        with contextlib.redirect_stderr(io.StringIO()):
            config = load_config(config_file or default_config_file(), config_file is not None)
    except CACHE_ERRORS:
        return []
    assert isinstance(config, Config)
    data_file = root.params.get("data_file") or config.data_file or default_data_file()
    return read_names(config, data_file, kind, archived)


def _matching(names: list[str], incomplete: str, prefix: str = "") -> list[CompletionItem]:
    lowered = incomplete.casefold()
    return [CompletionItem(prefix + n) for n in names if n.casefold().startswith(lowered)]


def names(kind: Kind, archived: bool = False) -> Completer:
    """Complete a plain project or tag name."""

    def complete(
        ctx: click.Context, param: click.Parameter, incomplete: str
    ) -> list[CompletionItem]:
        return _matching(_cached(ctx, kind, archived), incomplete)

    return complete


def tag_words(ctx: click.Context, param: click.Parameter, incomplete: str) -> list[CompletionItem]:
    """Complete ``+TAG`` words only."""
    if not incomplete.startswith("+"):
        return []
    return _matching(_cached(ctx, "tag", False), incomplete[1:], "+")


def classification(
    ctx: click.Context, param: click.Parameter, incomplete: str
) -> list[CompletionItem]:
    """Complete ``PROJECT [+TAG ...]``: a project word first, then ``+TAG`` words."""
    if incomplete.startswith("+"):
        return tag_words(ctx, param, incomplete)
    given = ctx.params.get(param.name or "") or ()
    if any(not word.startswith("+") for word in given):
        return []
    return _matching(_cached(ctx, "project", False), incomplete)
