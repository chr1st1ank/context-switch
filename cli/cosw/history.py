"""History commands: add, edit, remove, log, report."""

from __future__ import annotations

import json
from collections.abc import Callable
from typing import Any, TypeVar

import click
from contextswitch_core import Logbook, Span

from cosw.cli import main
from cosw.core import (
    describe,
    echo_created,
    parse_classification,
    parse_dt,
    project_name,
    read_logbook,
    require_tag,
    resolve_project_id,
    resolve_span,
    resolve_tag_ids,
    span_to_json,
    tag_names,
    transact,
)
from cosw.reporting import Filters, matching_spans, totals_by, totals_by_day
from cosw.timeparse import fmt_duration, fmt_local, fmt_time, parse_range_end, utcnow

_F = TypeVar("_F", bound=Callable[..., Any])


@main.command()
@click.argument("args", nargs=-1, metavar="PROJECT [+TAG ...]")
@click.option("--from", "from_str", required=True, metavar="WHEN", help="Span start time.")
@click.option(
    "--to",
    "to_str",
    required=True,
    metavar="WHEN",
    help="Span end time; a bare date means the end of that day.",
)
@click.pass_context
def add(ctx: click.Context, args: tuple[str, ...], from_str: str, to_str: str) -> None:
    """Record a span that was not tracked live."""
    project_name, tag_names = parse_classification(args)
    started = parse_dt(from_str)
    try:
        stopped = parse_range_end(to_str)
    except ValueError as e:
        raise click.BadParameter(str(e)) from e
    now = utcnow()
    created: list[str] = []

    def mutate(logbook: Logbook) -> str:
        project_id = resolve_project_id(logbook, project_name, now, created)
        tag_ids = resolve_tag_ids(logbook, tag_names, now, created)
        span_id = logbook.add_span(started, stopped, project_id, tag_ids, now)
        span = logbook.span(span_id)
        assert span is not None
        return describe(logbook, span)

    description = transact(ctx, mutate)
    echo_created(created)
    seconds = (stopped - started).total_seconds()
    click.echo(
        f"Recorded {description} {fmt_local(started)} – {fmt_time(stopped)} "
        f"({fmt_duration(seconds)})"
    )


def _parse_edit_args(args: tuple[str, ...]) -> tuple[str | None, list[str]]:
    """Split edit positionals into a span ref and +tag additions."""
    ref = None
    adds: list[str] = []
    for arg in args:
        if arg.startswith("+"):
            if len(arg) == 1:
                raise click.UsageError("'+' needs a tag name")
            adds.append(arg[1:])
        elif ref is None:
            ref = arg
        else:
            raise click.UsageError(f"unexpected argument {arg!r}")
    return ref, adds


@main.command(context_settings={"ignore_unknown_options": True})
@click.argument("args", nargs=-1, metavar="[SPAN] [+TAG ...]")
@click.option("--start", "start_str", metavar="WHEN", help="New start time.")
@click.option("--stop", "stop_str", metavar="WHEN", help="New stop time (stopped spans only).")
@click.option("--project", "project_name", metavar="NAME", help="Assign to a project.")
@click.option("--unassign", is_flag=True, help="Clear the span's project.")
@click.option("--untag", "untags", multiple=True, metavar="TAG", help="Remove a tag.")
@click.pass_context
def edit(
    ctx: click.Context,
    args: tuple[str, ...],
    start_str: str | None,
    stop_str: str | None,
    project_name: str | None,
    unassign: bool,
    untags: tuple[str, ...],
) -> None:
    """Edit a span's times, project, or tags.

    SPAN is a recency index (-1, -2, ...) or an ID prefix; it defaults to the
    active span, or the most recent span when idle.
    """
    ref, adds = _parse_edit_args(args)
    if project_name is not None and unassign:
        raise click.UsageError("--project and --unassign are mutually exclusive")
    if not any([start_str, stop_str, project_name, unassign, adds, untags]):
        raise click.UsageError("nothing to change")
    started = parse_dt(start_str) if start_str else None
    stopped = parse_dt(stop_str) if stop_str else None
    now = utcnow()
    created: list[str] = []

    def mutate(logbook: Logbook) -> str:
        span = resolve_span(logbook, ref)
        if unassign:
            logbook.unassign_span(span.id, now)
        project_id = None
        if project_name is not None:
            project_id = resolve_project_id(logbook, project_name, now, created)
        tag_ids = None
        if adds or untags:
            current = list(span.tag_ids)
            for name in adds:
                tag_id = resolve_tag_ids(logbook, [name], now, created)[0]
                if tag_id not in current:
                    current.append(tag_id)
            for name in untags:
                tag = require_tag(logbook, name)
                if tag.id not in current:
                    raise click.ClickException(f"span has no tag {name!r}")
                current.remove(tag.id)
            tag_ids = current
        logbook.edit_span(
            span.id,
            now,
            started_at=started,
            stopped_at=stopped,
            project_id=project_id,
            tag_ids=tag_ids,
        )
        updated = logbook.span(span.id)
        assert updated is not None
        return describe(logbook, updated)

    description = transact(ctx, mutate)
    echo_created(created)
    click.echo(f"Updated {description}")


@main.command(context_settings={"ignore_unknown_options": True})
@click.argument("ref", metavar="SPAN")
@click.option("-f", "--force", is_flag=True, help="Remove without confirmation.")
@click.pass_context
def remove(ctx: click.Context, ref: str, force: bool) -> None:
    """Remove a span entirely."""
    now = utcnow()

    def mutate(logbook: Logbook) -> str:
        span = resolve_span(logbook, ref)
        end = span.stopped_at if span.stopped_at is not None else now
        description = (
            f"{describe(logbook, span)} {fmt_local(span.started_at)} – "
            f"{fmt_time(end)} ({fmt_duration((end - span.started_at).total_seconds())})"
        )
        logbook.remove_span(span.id)
        return description

    def confirm(description: str) -> None:
        if not force:
            click.confirm(f"Remove span {description}?", abort=True)

    description = transact(ctx, mutate, confirm=confirm)
    click.echo(f"Removed {description}")


def _filter_options(command: _F) -> _F:
    """Attach the shared log/report filter options to a command."""
    options = [
        click.option(
            "--range",
            "range_",
            type=click.Choice(["day", "week", "month", "year", "all"]),
            help="Shortcut for --from: day, week, month, year, or all.",
        ),
        click.option("--from", "from_str", metavar="WHEN", help="Range start."),
        click.option("--to", "to_str", metavar="WHEN", help="Range end (inclusive)."),
        click.option("-p", "--project", "projects", multiple=True, help="Only this project."),
        click.option(
            "-T",
            "--tag",
            "tags",
            multiple=True,
            help="Only spans having all of these tags.",
        ),
        click.option(
            "--ignore-project", "ignore_projects", multiple=True, help="Exclude this project."
        ),
        click.option(
            "--ignore-tag", "ignore_tags", multiple=True, help="Exclude spans with this tag."
        ),
    ]
    for option in reversed(options):
        command = option(command)
    return command


def _filters(
    range_: str | None,
    from_str: str | None,
    to_str: str | None,
    projects: tuple[str, ...],
    tags: tuple[str, ...],
    ignore_projects: tuple[str, ...],
    ignore_tags: tuple[str, ...],
    include_current: bool,
) -> Filters:
    return Filters(
        range=range_,
        from_str=from_str,
        to_str=to_str,
        projects=projects,
        tags=tags,
        ignore_projects=ignore_projects,
        ignore_tags=ignore_tags,
        include_current=include_current,
    )


@main.command()
@_filter_options
@click.option("-r", "--reverse", is_flag=True, help="Oldest first.")
@click.option(
    "-c/-C",
    "--current/--no-current",
    default=True,
    help="Include the active timer (default: include).",
)
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def log(
    ctx: click.Context,
    range_: str | None,
    from_str: str | None,
    to_str: str | None,
    projects: tuple[str, ...],
    tags: tuple[str, ...],
    ignore_projects: tuple[str, ...],
    ignore_tags: tuple[str, ...],
    reverse: bool,
    current: bool,
    as_json: bool,
) -> None:
    """List recorded spans, newest first."""
    logbook = read_logbook(ctx)
    now = utcnow()
    filters = _filters(
        range_, from_str, to_str, projects, tags, ignore_projects, ignore_tags, current
    )
    rows = matching_spans(logbook, filters, now)
    if not reverse:
        rows.reverse()
    if as_json:
        payload = [
            {
                **span_to_json(logbook, span, now),
                "started_at": lo.isoformat(),
                "stopped_at": hi.isoformat() if not span.is_active else None,
                "seconds": int((hi - lo).total_seconds()),
            }
            for span, lo, hi in rows
        ]
        click.echo(json.dumps(payload))
        return
    for span, lo, hi in rows:
        end = "now" if span.is_active else fmt_time(hi)
        seconds = (hi - lo).total_seconds()
        click.echo(
            f"{span.id[:8]}  {fmt_local(lo)} – {end:<16}  "
            f"{describe(logbook, span)}  {fmt_duration(seconds)}"
        )


def _totals_by_project(logbook: Logbook):
    def key(span: Span) -> list[str]:
        return [project_name(logbook, span)]

    return key


def _totals_by_tag(logbook: Logbook):
    def key(span: Span) -> list[str]:
        names = tag_names(logbook, span)
        return names if names else ["(untagged)"]

    return key


@main.command()
@_filter_options
@click.option(
    "-c/-C",
    "--current/--no-current",
    default=True,
    help="Include the active timer (default: include).",
)
@click.option(
    "--by",
    type=click.Choice(["project", "tag", "day"]),
    default="project",
    show_default=True,
    help="Aggregation dimension.",
)
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def report(
    ctx: click.Context,
    range_: str | None,
    from_str: str | None,
    to_str: str | None,
    projects: tuple[str, ...],
    tags: tuple[str, ...],
    ignore_projects: tuple[str, ...],
    ignore_tags: tuple[str, ...],
    current: bool,
    by: str,
    as_json: bool,
) -> None:
    """Report time totals by project, tag, or day."""
    logbook = read_logbook(ctx)
    now = utcnow()
    filters = _filters(
        range_, from_str, to_str, projects, tags, ignore_projects, ignore_tags, current
    )
    rows = matching_spans(logbook, filters, now)
    total = sum((hi - lo).total_seconds() for _, lo, hi in rows)

    if by == "day":
        days = totals_by_day(logbook, rows)
        if as_json:
            payload = {
                "by": "day",
                "days": [
                    {
                        "date": day,
                        "totals": {name: int(secs) for name, secs in totals.items()},
                        "total_seconds": int(sum(totals.values())),
                    }
                    for day, totals in sorted(days.items())
                ],
                "total_seconds": int(total),
            }
            click.echo(json.dumps(payload))
            return
        width = max(
            (len(name) for totals in days.values() for name in totals), default=len("total")
        )
        width = max(width, len("total"))
        for day, day_totals in sorted(days.items()):
            click.echo(day)
            for name, secs in sorted(day_totals.items(), key=lambda kv: -kv[1]):
                click.echo(f"  {name:<{width}}  {fmt_duration(secs)}")
            click.echo(f"  {'total':<{width}}  {fmt_duration(sum(day_totals.values()))}")
        if not days:
            click.echo("No time recorded.")
        return

    key = _totals_by_tag(logbook) if by == "tag" else _totals_by_project(logbook)
    totals = totals_by(rows, key)
    if as_json:
        payload = {
            "by": by,
            "totals": {name: int(secs) for name, secs in totals.items()},
            "total_seconds": int(total),
        }
        click.echo(json.dumps(payload))
        return
    if not totals:
        click.echo("No time recorded.")
        return
    width = max(len(name) for name in totals)
    width = max(width, len("total"))
    for name, secs in sorted(totals.items(), key=lambda kv: -kv[1]):
        click.echo(f"{name:<{width}}  {fmt_duration(secs)}")
    click.echo(f"{'total':<{width}}  {fmt_duration(total)}")
