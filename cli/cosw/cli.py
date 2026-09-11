"""cosw — context-switch command-line client."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import click
from click.core import ParameterSource
from contextswitch_core import Document

from cosw.core import (
    ENV_DATA_FILE,
    default_data_file,
    describe,
    echo_created,
    open_provider,
    parse_classification,
    parse_dt,
    read_document,
    resolve_project_id,
    resolve_span,
    resolve_tag_ids,
    sorted_spans,
    span_to_json,
    storage_info,
    transact,
)
from cosw.timeparse import fmt_duration, fmt_time, utcnow


@click.group(context_settings={"help_option_names": ["-h", "--help"]})
@click.option(
    "--data-file",
    type=click.Path(dir_okay=False, path_type=Path),
    envvar=ENV_DATA_FILE,
    help="Path to the canonical data file.",
)
@click.version_option()
@click.pass_context
def main(ctx: click.Context, data_file: Path | None) -> None:
    """context-switch time tracking CLI."""
    ctx.ensure_object(dict)
    if data_file is None:
        path = default_data_file()
        origin = "default"
    else:
        path = data_file
        source = ctx.get_parameter_source("data_file")
        origin = "--data-file" if source == ParameterSource.COMMANDLINE else ENV_DATA_FILE
    ctx.obj["data_file"] = path
    ctx.obj["data_file_origin"] = origin
    ctx.obj["provider"] = open_provider(path)


def _resolve_all(
    doc: Document,
    project_name: str | None,
    tag_names: list[str],
    at,
    created: list[str],
) -> tuple[str | None, list[str]]:
    project_id = resolve_project_id(doc, project_name, at, created)
    tag_ids = resolve_tag_ids(doc, tag_names, at, created)
    return project_id, tag_ids


@main.command()
@click.argument("args", nargs=-1, metavar="PROJECT [+TAG ...]")
@click.option("--at", "at_str", metavar="WHEN", help="Start time (ISO 8601 or HH:MM); default now.")
@click.pass_context
def start(ctx: click.Context, args: tuple[str, ...], at_str: str | None) -> None:
    """Start a new timer for PROJECT with optional +TAGs."""
    project_name, tag_names = parse_classification(args)
    at = parse_dt(at_str) if at_str else utcnow()
    created: list[str] = []

    def mutate(doc: Document) -> str:
        if doc.active_span() is not None:
            raise click.ClickException(
                "a timer is already active; use 'cosw switch' to change tasks"
            )
        project_id, tag_ids = _resolve_all(doc, project_name, tag_names, at, created)
        doc.start_timer(at, project_id, tag_ids)
        active = doc.active_span()
        assert active is not None
        return describe(doc, active)

    description = transact(ctx, mutate)
    echo_created(created)
    click.echo(f"Timer started on {description} at {fmt_time(at)}")


@main.command()
@click.option("--at", "at_str", metavar="WHEN", help="Stop time (ISO 8601 or HH:MM); default now.")
@click.pass_context
def stop(ctx: click.Context, at_str: str | None) -> None:
    """Stop the active timer."""
    at = parse_dt(at_str) if at_str else utcnow()

    def mutate(doc: Document) -> tuple[str, float]:
        active = doc.active_span()
        if active is None:
            raise click.ClickException("no active timer")
        description = describe(doc, active)
        seconds = (at - active.started_at).total_seconds()
        doc.stop_timer(at)
        return description, seconds

    description, seconds = transact(ctx, mutate)
    click.echo(f"Stopped {description} at {fmt_time(at)} ({fmt_duration(seconds)})")


@main.command()
@click.argument("args", nargs=-1, metavar="PROJECT [+TAG ...]")
@click.option(
    "--at", "at_str", metavar="WHEN", help="Switch time (ISO 8601 or HH:MM); default now."
)
@click.pass_context
def switch(ctx: click.Context, args: tuple[str, ...], at_str: str | None) -> None:
    """Stop the active timer and start a new one in one action."""
    project_name, tag_names = parse_classification(args)
    at = parse_dt(at_str) if at_str else utcnow()
    created: list[str] = []

    def mutate(doc: Document) -> tuple[str | None, str]:
        previous = doc.active_span()
        previous_desc = describe(doc, previous) if previous is not None else None
        project_id, tag_ids = _resolve_all(doc, project_name, tag_names, at, created)
        doc.switch(at, project_id, tag_ids)
        active = doc.active_span()
        assert active is not None
        return previous_desc, describe(doc, active)

    previous_desc, description = transact(ctx, mutate)
    echo_created(created)
    if previous_desc is not None:
        click.echo(f"Switched from {previous_desc} to {description} at {fmt_time(at)}")
    else:
        click.echo(f"Timer started on {description} at {fmt_time(at)}")


@main.command(context_settings={"ignore_unknown_options": True})
@click.argument("ref", required=False, metavar="SPAN")
@click.option(
    "--at", "at_str", metavar="WHEN", help="Resume time (ISO 8601 or HH:MM); default now."
)
@click.pass_context
def resume(ctx: click.Context, ref: str | None, at_str: str | None) -> None:
    """Start a copy of a previous span, typically after a break.

    SPAN is a recency index (-1, -2, ...) or an ID prefix; it defaults to the
    most recent span.
    """
    at = parse_dt(at_str) if at_str else utcnow()

    def mutate(doc: Document) -> str:
        if doc.active_span() is not None:
            raise click.ClickException(
                "a timer is already active; use 'cosw switch' to change tasks"
            )
        if not sorted_spans(doc):
            raise click.ClickException("nothing to resume")
        source = resolve_span(doc, ref)
        doc.start_timer(at, source.project_id, source.tag_ids)
        active = doc.active_span()
        assert active is not None
        return describe(doc, active)

    description = transact(ctx, mutate)
    click.echo(f"Resumed {description} at {fmt_time(at)}")


@main.command()
@click.option("-f", "--force", is_flag=True, help="Discard without confirmation.")
@click.pass_context
def cancel(ctx: click.Context, force: bool) -> None:
    """Discard the active timer without recording the time."""
    now = utcnow()

    def mutate(doc: Document) -> tuple[str, float]:
        active = doc.active_span()
        if active is None:
            raise click.ClickException("no active timer")
        description = describe(doc, active)
        seconds = (now - active.started_at).total_seconds()
        doc.remove_span(active.id)
        return description, seconds

    def confirm(result: tuple[str, float]) -> None:
        if not force:
            click.confirm(f"Discard timer on {result[0]}?", abort=True)

    description, seconds = transact(ctx, mutate, confirm=confirm)
    click.echo(f"Discarded timer on {description} ({fmt_duration(seconds)} not recorded)")


@main.command()
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.option("-v", "--verbose", is_flag=True, help="Also show storage configuration.")
@click.pass_context
def status(ctx: click.Context, as_json: bool, verbose: bool) -> None:
    """Show the active timer and elapsed time."""
    doc = read_document(ctx)
    active = doc.active_span()
    now = utcnow()
    if as_json:
        payload: dict[str, Any] = (
            span_to_json(doc, active, now) if active is not None else {"active": False}
        )
        if verbose:
            payload["storage"] = storage_info(ctx)
        click.echo(json.dumps(payload))
        return
    if verbose:
        info = storage_info(ctx)
        click.echo(f"storage: {info['url']} (via {info['source']})")
        click.echo()
    if active is None:
        click.echo("No active timer.")
    else:
        elapsed = (now - active.started_at).total_seconds()
        click.echo(
            f"{describe(doc, active)} — started {fmt_time(active.started_at)}, "
            f"elapsed {fmt_duration(elapsed)}"
        )


# Command modules register themselves on ``main`` when imported.
from cosw import history as _history  # noqa: E402, F401
from cosw import manage as _manage  # noqa: E402, F401

if __name__ == "__main__":
    main()
