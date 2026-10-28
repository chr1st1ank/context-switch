"""Shared history-query options and error translation for ``log``, ``report`` and ``export``."""

from __future__ import annotations

from collections.abc import Callable
from datetime import datetime
from typing import Any, TypeVar

import click
from contextswitch_core import Logbook, Span

from cosw import completers
from cosw.reporting import (
    RANGE_SHORTCUTS,
    Filters,
    RangeConflictError,
    UnknownNameError,
    matching_spans,
)

_F = TypeVar("_F", bound=Callable[..., Any])


def matching(
    logbook: Logbook, filters: Filters, now: datetime
) -> list[tuple[Span, datetime, datetime]]:
    """Run the shared span query, translating query errors into CLI errors."""
    try:
        return matching_spans(logbook, filters, now)
    except RangeConflictError as e:
        raise click.UsageError(str(e)) from e
    except UnknownNameError as e:
        raise click.ClickException(str(e)) from e
    except ValueError as e:
        raise click.BadParameter(str(e)) from e


def filter_options(command: _F) -> _F:
    """Attach the shared log/report/export filter options to a command."""
    options = [
        click.option(
            "--range",
            "range_",
            type=click.Choice(list(RANGE_SHORTCUTS)),
            help="Shortcut for --from: day, week, month, year, or all.",
        ),
        click.option("--from", "from_str", metavar="WHEN", help="Range start."),
        click.option("--to", "to_str", metavar="WHEN", help="Range end (inclusive)."),
        click.option(
            "-p",
            "--project",
            "projects",
            multiple=True,
            help="Only this project.",
            shell_complete=completers.names("project"),
        ),
        click.option(
            "-T",
            "--tag",
            "tags",
            multiple=True,
            help="Only spans having all of these tags.",
            shell_complete=completers.names("tag"),
        ),
        click.option(
            "--client",
            "clients",
            multiple=True,
            metavar="CLIENT",
            help="Only spans whose project carries this client label.",
        ),
        click.option(
            "--ignore-project",
            "ignore_projects",
            multiple=True,
            help="Exclude this project.",
            shell_complete=completers.names("project"),
        ),
        click.option(
            "--ignore-tag",
            "ignore_tags",
            multiple=True,
            help="Exclude spans with this tag.",
            shell_complete=completers.names("tag"),
        ),
        click.option(
            "--ignore-client",
            "ignore_clients",
            multiple=True,
            metavar="CLIENT",
            help="Exclude spans whose project carries this client label.",
        ),
    ]
    for option in reversed(options):
        command = option(command)
    return command


def build_filters(
    range_: str | None,
    from_str: str | None,
    to_str: str | None,
    projects: tuple[str, ...],
    tags: tuple[str, ...],
    clients: tuple[str, ...],
    ignore_projects: tuple[str, ...],
    ignore_tags: tuple[str, ...],
    ignore_clients: tuple[str, ...],
    include_current: bool,
) -> Filters:
    return Filters(
        range=range_,
        from_str=from_str,
        to_str=to_str,
        projects=projects,
        tags=tags,
        clients=clients,
        ignore_projects=ignore_projects,
        ignore_tags=ignore_tags,
        ignore_clients=ignore_clients,
        include_current=include_current,
    )
