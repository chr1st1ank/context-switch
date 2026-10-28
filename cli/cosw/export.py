"""The ``cosw export`` command: standalone timesheets in markdown or PDF."""

from __future__ import annotations

from pathlib import Path

import click

from cosw.cli import main
from cosw.core import read_logbook
from cosw.filters import build_filters, filter_options, matching
from cosw.reporting import bounds
from cosw.timeparse import utcnow
from cosw.timesheet import (
    COLUMNS,
    DEFAULT_COLUMNS,
    LANGS,
    build_timesheet,
    render_markdown,
    render_pdf,
)

_FORMATS = {"markdown": (".md", ".markdown"), "pdf": (".pdf",)}


def _parse_columns(value: str | None) -> tuple[str, ...]:
    if value is None:
        return DEFAULT_COLUMNS
    columns = tuple(part.strip() for part in value.split(","))
    if not columns or not all(columns) or len(set(columns)) != len(columns):
        raise click.BadParameter(
            f"invalid column list {value!r}; pass a comma-separated subset of: "
            + ", ".join(COLUMNS)
        )
    unknown = [column for column in columns if column not in COLUMNS]
    if unknown:
        raise click.BadParameter(
            f"unknown columns: {', '.join(unknown)}; valid: {', '.join(COLUMNS)}"
        )
    return columns


def _resolve_format(fmt: str | None, output: Path | None) -> str:
    if fmt is not None:
        return fmt
    if output is not None:
        suffix = output.suffix.lower()
        for name, extensions in _FORMATS.items():
            if suffix in extensions:
                return name
        raise click.UsageError(f"cannot infer format from extension {suffix!r}; pass --format")
    return "markdown"


@main.command()
@filter_options
@click.option(
    "-c/-C",
    "--current/--no-current",
    default=False,
    help="Include the active timer (default: exclude).",
)
@click.option("--name", metavar="NAME", help="Full name printed in the timesheet header.")
@click.option(
    "--columns",
    "columns_str",
    metavar="LIST",
    help=f"Comma-separated columns and their order; any of: {', '.join(COLUMNS)}.",
)
@click.option(
    "--lang",
    type=click.Choice(list(LANGS)),
    default="en",
    show_default=True,
    help="Document language.",
)
@click.option(
    "--format",
    "fmt",
    type=click.Choice(list(_FORMATS)),
    default=None,
    help="Output format (default: markdown, or inferred from -o).",
)
@click.option(
    "--signature",
    "signature_lines",
    is_flag=True,
    help="Draw blank signature lines (place/date + signature) below the totals.",
)
@click.option(
    "--signature-image",
    type=click.Path(exists=True, dir_okay=False, path_type=Path),
    metavar="PNG",
    help="Draw the signature lines with this image embedded at the signature.",
)
@click.option(
    "-o",
    "--output",
    type=click.Path(dir_okay=False, path_type=Path),
    metavar="FILE",
    help="Write to FILE instead of stdout.",
)
@click.pass_context
def export(
    ctx: click.Context,
    range_: str | None,
    from_str: str | None,
    to_str: str | None,
    projects: tuple[str, ...],
    tags: tuple[str, ...],
    clients: tuple[str, ...],
    ignore_projects: tuple[str, ...],
    ignore_tags: tuple[str, ...],
    ignore_clients: tuple[str, ...],
    current: bool,
    name: str | None,
    columns_str: str | None,
    lang: str,
    fmt: str | None,
    signature_lines: bool,
    signature_image: Path | None,
    output: Path | None,
) -> None:
    """Export a standalone timesheet (markdown to stdout, PDF via -o)."""
    columns = _parse_columns(columns_str)
    fmt = _resolve_format(fmt, output)
    if fmt == "pdf" and output is None:
        raise click.UsageError("PDF output requires -o FILE")
    if signature_lines and signature_image is not None:
        raise click.UsageError("use either --signature or --signature-image")
    if fmt == "markdown" and (signature_lines or signature_image is not None):
        raise click.UsageError("--signature/--signature-image are only supported for PDF output")
    logbook = read_logbook(ctx)
    now = utcnow()
    filters = build_filters(
        range_,
        from_str,
        to_str,
        projects,
        tags,
        clients,
        ignore_projects,
        ignore_tags,
        ignore_clients,
        current,
    )
    rows = matching(logbook, filters, now)
    if not rows:
        raise click.ClickException("no spans matched the filters")
    lo, hi = bounds(filters, now)
    sheet = build_timesheet(
        logbook,
        rows,
        name=name,
        client=clients[0] if len(clients) == 1 else None,
        columns=columns,
        lang=lang,
        period_from=lo.astimezone().date() if lo is not None else None,
        period_to=hi.astimezone().date() if hi is not None else None,
        now=now,
        signature_lines=signature_lines or signature_image is not None,
        signature_image=signature_image,
    )
    if fmt == "pdf":
        assert output is not None
        try:
            render_pdf(sheet, output)
        except OSError as e:
            raise click.ClickException(f"cannot write {output}: {e}") from e
    elif output is not None:
        try:
            output.write_text(render_markdown(sheet), encoding="utf-8")
        except OSError as e:
            raise click.ClickException(f"cannot write {output}: {e}") from e
    else:
        click.echo(render_markdown(sheet), nl=False)
        return
    click.echo(f"Wrote {output}")
