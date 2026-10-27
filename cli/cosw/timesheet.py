"""Timesheet document model and renderers for ``cosw export``.

Pure document logic with no CLI-framework imports: ``build_timesheet`` turns
filtered, range-clipped spans into a language-aware document structure, and
``render_markdown`` / ``render_pdf`` serialize it.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime, time
from pathlib import Path
from typing import Any

from contextswitch_core import Logbook, Span

from cosw.core import client_name, tag_names
from cosw.reporting import day_segments

COLUMNS = (
    "date",
    "start",
    "end",
    "hours",
    "project",
    "client",
    "tags",
    "description",
    "id",
)
DEFAULT_COLUMNS = ("date", "start", "end", "hours", "project", "description")
LANGS = ("en", "de")

_STRINGS: dict[str, dict[str, Any]] = {
    "en": {
        "title": "Timesheet",
        "name": "Name",
        "period": "Period",
        "generated": "Generated",
        "totals": "Totals",
        "total": "Total",
        "headings": {
            "date": "Date",
            "start": "Start",
            "end": "End",
            "hours": "Hours",
            "project": "Project",
            "client": "Client",
            "tags": "Tags",
            "description": "Description",
            "id": "ID",
        },
    },
    "de": {
        "title": "Stundenzettel",
        "name": "Name",
        "period": "Zeitraum",
        "generated": "Erstellt am",
        "totals": "Summen",
        "total": "Gesamt",
        "headings": {
            "date": "Datum",
            "start": "Beginn",
            "end": "Ende",
            "hours": "Stunden",
            "project": "Projekt",
            "client": "Kunde",
            "tags": "Tags",
            "description": "Beschreibung",
            "id": "ID",
        },
    },
}


@dataclass
class Row:
    """One table line: a span segment confined to a single local date."""

    day: date
    start: datetime
    end: datetime
    seconds: float
    project: str | None
    client: str | None
    tags: list[str]
    description: str | None
    span_id: str


@dataclass
class Timesheet:
    """A fully localized timesheet document, ready to render."""

    lang: str
    name: str | None
    client: str | None
    generated: date
    period_from: date
    period_to: date
    columns: tuple[str, ...]
    rows: list[Row]
    subtotals: list[tuple[str, float]]
    total_seconds: float


def _project_name(logbook: Logbook, span: Span) -> str | None:
    if span.project_id is None:
        return None
    project = logbook.project(span.project_id)
    return project.name if project is not None else None


def build_timesheet(
    logbook: Logbook,
    matches: list[tuple[Span, datetime, datetime]],
    *,
    name: str | None,
    client: str | None,
    columns: tuple[str, ...],
    lang: str,
    period_from: date | None,
    period_to: date | None,
    now: datetime,
) -> Timesheet:
    """Assemble a Timesheet from range-clipped spans (as ``matching_spans`` returns).

    Spans are split at local midnight so every row belongs to one date.
    ``period_from``/``period_to`` default to the data's own bounds when the
    filter left that side open. ``client`` is only shown when the filter
    already narrowed the data to a single client.
    """
    rows: list[Row] = []
    subtotal_seconds: dict[str, float] = {}
    for span, lo, hi in matches:
        project = _project_name(logbook, span)
        span_client = client_name(logbook, span)
        tags = tag_names(logbook, span)
        if project is None:
            label = ""
        elif span_client is not None:
            label = f"{project} ({span_client})"
        else:
            label = project
        for seg_lo, seg_hi in day_segments(lo, hi):
            seconds = (seg_hi - seg_lo).total_seconds()
            rows.append(
                Row(
                    day=seg_lo.astimezone().date(),
                    start=seg_lo,
                    end=seg_hi,
                    seconds=seconds,
                    project=project,
                    client=span_client,
                    tags=tags,
                    description=span.description,
                    span_id=span.id[:8],
                )
            )
            subtotal_seconds[label] = subtotal_seconds.get(label, 0.0) + seconds
    days = [row.day for row in rows]
    if client is not None:
        # Show the canonical client label, not the filter's casing.
        seen = {row.client for row in rows}
        if len(seen) == 1:
            client = seen.pop()
    return Timesheet(
        lang=lang,
        name=name,
        client=client,
        generated=now.astimezone().date(),
        period_from=period_from or min(days),
        period_to=period_to or max(days),
        columns=columns,
        rows=rows,
        subtotals=sorted(subtotal_seconds.items(), key=lambda kv: (-kv[1], kv[0].casefold())),
        total_seconds=sum(subtotal_seconds.values()),
    )


def _fmt_date(day: date, lang: str) -> str:
    if lang == "de":
        return day.strftime("%d.%m.%Y")
    return day.isoformat()


def _fmt_hours(seconds: float, lang: str) -> str:
    text = f"{seconds / 3600:.2f}"
    return text.replace(".", ",") if lang == "de" else text


def _fmt_start(dt: datetime) -> str:
    return dt.astimezone().strftime("%H:%M")


def _fmt_end(dt: datetime) -> str:
    local = dt.astimezone()
    return "24:00" if local.time() == time.min else local.strftime("%H:%M")


def _oneline(text: str) -> str:
    return " ".join(text.split())


def _cell(row: Row, column: str, lang: str) -> str:
    if column == "date":
        return _fmt_date(row.day, lang)
    if column == "start":
        return _fmt_start(row.start)
    if column == "end":
        return _fmt_end(row.end)
    if column == "hours":
        return _fmt_hours(row.seconds, lang)
    if column == "project":
        return row.project or "—"
    if column == "client":
        return row.client or "—"
    if column == "tags":
        return ", ".join(row.tags)
    if column == "description":
        return _oneline(row.description or "")
    return row.span_id


def _md_escape(text: str) -> str:
    return text.replace("|", "\\|")


def render_markdown(sheet: Timesheet) -> str:
    """Render the timesheet as a standalone markdown document."""
    strings = _STRINGS[sheet.lang]
    lines = [
        f"# {strings['title']}",
        "",
        f"**{strings['name']}:** {sheet.name or ''}",
        (
            f"**{strings['period']}:** {_fmt_date(sheet.period_from, sheet.lang)}"
            f" – {_fmt_date(sheet.period_to, sheet.lang)}"
        ),
    ]
    if sheet.client is not None:
        lines.append(f"**{strings['headings']['client']}:** {sheet.client}")
    lines += [
        f"**{strings['generated']}:** {_fmt_date(sheet.generated, sheet.lang)}",
        "",
        "| " + " | ".join(strings["headings"][c] for c in sheet.columns) + " |",
        "|" + "|".join("---:" if c == "hours" else "---" for c in sheet.columns) + "|",
    ]
    for row in sheet.rows:
        cells = [_md_escape(_cell(row, c, sheet.lang)) for c in sheet.columns]
        lines.append("| " + " | ".join(cells) + " |")
    lines += [
        "",
        f"## {strings['totals']}",
        "",
        f"| {strings['headings']['project']} | {strings['headings']['hours']} |",
        "|---|---:|",
    ]
    for label, seconds in sheet.subtotals:
        lines.append(f"| {_md_escape(label)} | {_fmt_hours(seconds, sheet.lang)} |")
    lines.append(f"| **{strings['total']}** | **{_fmt_hours(sheet.total_seconds, sheet.lang)}** |")
    return "\n".join(lines) + "\n"


_PDF_TRANSLATE = str.maketrans({"—": "-", "–": "-", "…": "..."})
_PDF_WEIGHTS = {
    "date": 22,
    "start": 14,
    "end": 14,
    "hours": 16,
    "project": 32,
    "client": 30,
    "tags": 28,
    "description": 60,
    "id": 18,
}


def _pdf_text(text: str) -> str:
    """Make text safe for the PDF core fonts (WinAnsi, roughly latin-1)."""
    return text.translate(_PDF_TRANSLATE).encode("latin-1", "replace").decode("latin-1")


def render_pdf(sheet: Timesheet, path: Path) -> None:
    """Render the timesheet as an A4 PDF document."""
    from fpdf import FPDF
    from fpdf.fonts import FontFace

    strings = _STRINGS[sheet.lang]
    pdf = FPDF(orientation="P", unit="mm", format="A4")
    pdf.set_auto_page_break(auto=True, margin=15)
    pdf.add_page()

    pdf.set_font("helvetica", style="B", size=16)
    pdf.cell(0, 10, strings["title"], new_x="LMARGIN", new_y="NEXT")
    pdf.set_font("helvetica", size=11)
    pdf.cell(
        0,
        7,
        f"{strings['name']}: {_pdf_text(sheet.name or '')}",
        new_x="LMARGIN",
        new_y="NEXT",
    )
    period = (
        f"{_fmt_date(sheet.period_from, sheet.lang)} - {_fmt_date(sheet.period_to, sheet.lang)}"
    )
    pdf.cell(0, 7, f"{strings['period']}: {period}", new_x="LMARGIN", new_y="NEXT")
    if sheet.client is not None:
        pdf.cell(
            0,
            7,
            f"{strings['headings']['client']}: {_pdf_text(sheet.client)}",
            new_x="LMARGIN",
            new_y="NEXT",
        )
    pdf.cell(
        0,
        7,
        f"{strings['generated']}: {_fmt_date(sheet.generated, sheet.lang)}",
        new_x="LMARGIN",
        new_y="NEXT",
    )
    pdf.ln(4)

    usable = pdf.w - pdf.l_margin - pdf.r_margin
    widths = tuple(_PDF_WEIGHTS[c] for c in sheet.columns)
    aligns = ["RIGHT" if c == "hours" else "LEFT" for c in sheet.columns]

    pdf.set_font(size=9)
    with pdf.table(
        col_widths=widths,
        text_align=tuple(aligns),
        headings_style=FontFace(emphasis="BOLD"),
        width=usable,
    ) as table:
        header = table.row()
        for column in sheet.columns:
            header.cell(strings["headings"][column])
        for sheet_row in sheet.rows:
            pdf_row = table.row()
            for column in sheet.columns:
                pdf_row.cell(_pdf_text(_cell(sheet_row, column, sheet.lang)))

    pdf.ln(6)
    pdf.set_font("helvetica", style="B", size=12)
    pdf.cell(0, 8, strings["totals"], new_x="LMARGIN", new_y="NEXT")
    pdf.set_font(size=9)
    with pdf.table(
        col_widths=(6, 1),
        text_align=("LEFT", "RIGHT"),
        headings_style=FontFace(emphasis="BOLD"),
        width=usable,
    ) as table:
        header = table.row()
        header.cell(strings["headings"]["project"])
        header.cell(strings["headings"]["hours"])
        for label, seconds in sheet.subtotals:
            pdf_row = table.row()
            pdf_row.cell(_pdf_text(label))
            pdf_row.cell(_fmt_hours(seconds, sheet.lang))
        total_row = table.row()
        total_row.cell(strings["total"], style=FontFace(emphasis="BOLD"))
        total_row.cell(_fmt_hours(sheet.total_seconds, sheet.lang), style=FontFace(emphasis="BOLD"))

    pdf.output(str(path))
