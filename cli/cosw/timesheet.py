"""Timesheet document model and renderers for ``cosw export``.

Pure document logic with no CLI-framework imports: ``build_timesheet`` turns
filtered, range-clipped spans into a language-aware document structure, and
``render_markdown`` / ``render_pdf`` serialize it.
"""

from __future__ import annotations

from dataclasses import dataclass
from datetime import date, datetime, time
from pathlib import Path
from typing import Any, override

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
        "page": "Page",
        "signature": "Signature",
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
        "page": "Seite",
        "signature": "Unterschrift",
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
    signature_lines: bool = False
    signature_image: Path | None = None


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
    signature_lines: bool = False,
    signature_image: Path | None = None,
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
        signature_lines=signature_lines,
        signature_image=signature_image,
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


_FONT_DIR = Path(__file__).parent / "assets" / "fonts"
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
_PDF_ALIGN = {"date": "CENTER", "start": "CENTER", "end": "CENTER", "hours": "RIGHT"}

_SIGNATURE_H = 18
_SIGNATURE_MAX_W = 80
_BASELINE_DENSITY = 0.3

_INK = (30, 30, 30)
_GRAY = (110, 110, 110)
_RULE = (185, 185, 185)
_FILL = (246, 246, 246)
_FILL_DARK = (232, 232, 232)


def _table_style() -> dict[str, Any]:
    """Booktabs-style look for the main PDF table."""
    from fpdf.enums import TableBordersLayout, TableCellFillMode
    from fpdf.fonts import FontFace

    return {
        "borders_layout": TableBordersLayout.SINGLE_TOP_LINE,
        "headings_style": FontFace(emphasis="BOLD"),
        "cell_fill_color": _FILL,
        "cell_fill_mode": TableCellFillMode.EVEN_ROWS,
        "padding": (1.3, 2.4),
    }


def _prepare_signature(path: Path) -> tuple[Any, float]:
    """Crop the signature to its ink and locate its baseline.

    Returns the image and the baseline as a fraction of its height from the top.
    The baseline is the last row whose ink density is still a sizeable share of
    the densest row; sparser rows below it are descenders (the tail of a "g").
    """
    from PIL import Image, ImageChops

    img = Image.open(path).convert("RGBA")
    flat = Image.alpha_composite(Image.new("RGBA", img.size, (255, 255, 255, 255)), img)
    ink = ImageChops.invert(flat.convert("L"))
    bbox = ink.point(lambda v: 255 if v > 16 else 0).getbbox()
    if bbox is None:
        return img, 1.0
    img, ink = img.crop(bbox), ink.crop(bbox)
    height = ink.height
    column = ink.resize((1, height), Image.Resampling.BOX)
    rows = [float(column.getpixel((0, y))) for y in range(height)]  # ty: ignore[invalid-argument-type]
    window = max(1, height // 20)
    smooth = [
        sum(rows[max(0, y - window) : y + window + 1])
        / len(rows[max(0, y - window) : y + window + 1])
        for y in range(height)
    ]
    threshold = _BASELINE_DENSITY * max(smooth)
    baseline = max(y for y, density in enumerate(smooth) if density >= threshold)
    return img, (baseline + 1) / height


def render_pdf(sheet: Timesheet, path: Path) -> None:
    """Render the timesheet as an A4 PDF document."""
    from fpdf import FPDF

    strings = _STRINGS[sheet.lang]

    class SheetPDF(FPDF):
        """Adds the page-number footer on every page."""

        @override
        def footer(self) -> None:
            self.set_y(-16)
            self.set_font("carlito", "I", 9)
            self.set_text_color(*_GRAY)
            self.cell(0, 6, f"{strings['page']} {self.page_no()}/{{nb}}", align="R")

    pdf = SheetPDF(orientation="P", unit="mm", format="A4")
    pdf.set_margins(20, 18, 20)
    pdf.set_auto_page_break(auto=True, margin=22)
    for style, filename in (("", "Regular"), ("B", "Bold"), ("I", "Italic")):
        pdf.add_font("carlito", style, str(_FONT_DIR / f"Carlito-{filename}.ttf"))
    pdf.set_title(strings["title"])
    pdf.set_author(sheet.name or "context-switch")
    pdf.set_creator("cosw export")
    pdf.set_lang("de-DE" if sheet.lang == "de" else "en-US")
    pdf.add_page()

    pdf.set_font("carlito", "B", 22)
    pdf.set_text_color(*_INK)
    pdf.cell(0, 13, strings["title"], new_x="LMARGIN", new_y="NEXT")
    pdf.ln(1)

    pdf.set_font("carlito", "", 10.5)
    pdf.set_text_color(*_GRAY)
    generated = f"{strings['generated']}: {_fmt_date(sheet.generated, sheet.lang)}"
    pdf.cell(0, 6, generated, align="R", new_x="LMARGIN", new_y="NEXT")
    pdf.ln(1)

    period = (
        f"{_fmt_date(sheet.period_from, sheet.lang)} – {_fmt_date(sheet.period_to, sheet.lang)}"
    )
    info = [(strings["name"], sheet.name or ""), (strings["period"], period)]
    if sheet.client is not None:
        info.append((strings["headings"]["client"], sheet.client))
    for label, value in info:
        pdf.set_text_color(*_GRAY)
        pdf.cell(26, 6, f"{label}:")
        pdf.set_text_color(*_INK)
        pdf.cell(0, 6, value, new_x="LMARGIN", new_y="NEXT")

    pdf.ln(8)

    usable = pdf.w - pdf.l_margin - pdf.r_margin
    widths = tuple(_PDF_WEIGHTS[c] for c in sheet.columns)
    aligns = tuple(_PDF_ALIGN.get(c, "LEFT") for c in sheet.columns)

    pdf.set_font("carlito", "", 10)
    pdf.set_text_color(*_INK)
    pdf.set_draw_color(*_RULE)
    pdf.set_line_width(0.15)
    with pdf.table(
        col_widths=widths,
        text_align=aligns,
        width=usable,
        **_table_style(),
    ) as table:
        header = table.row()
        for column in sheet.columns:
            header.cell(strings["headings"][column])
        for sheet_row in sheet.rows:
            pdf_row = table.row()
            for column in sheet.columns:
                pdf_row.cell(_cell(sheet_row, column, sheet.lang))

    # Totals block aligned with the table's right edge, kept on one page.
    from fpdf.enums import Align, TableBordersLayout
    from fpdf.fonts import FontFace

    block_w = 80
    estimate = 14 + (len(sheet.subtotals) + 2) * 7
    if pdf.get_y() + estimate > pdf.h - pdf.b_margin:
        pdf.add_page()
    pdf.ln(8)
    pdf.set_font("carlito", "B", 12)
    pdf.set_text_color(*_INK)
    pdf.set_x(pdf.w - pdf.r_margin - block_w)
    pdf.cell(block_w, 7, strings["totals"], new_x="LMARGIN", new_y="NEXT")
    pdf.ln(1)
    pdf.set_font("carlito", "", 10)
    with pdf.table(
        borders_layout=TableBordersLayout.SINGLE_TOP_LINE,
        col_widths=(4, 1.4),
        text_align=("LEFT", "RIGHT"),
        headings_style=FontFace(emphasis="BOLD"),
        padding=(1.2, 2.4),
        width=block_w,
        align=Align.R,
    ) as table:
        header = table.row()
        header.cell(strings["headings"]["project"])
        header.cell(strings["headings"]["hours"])
        for label, seconds in sheet.subtotals:
            pdf_row = table.row()
            pdf_row.cell(label)
            pdf_row.cell(_fmt_hours(seconds, sheet.lang))
        total_row = table.row()
        emphasis = FontFace(emphasis="BOLD", fill_color=_FILL_DARK)
        total_row.cell(strings["total"], style=emphasis)
        total_row.cell(_fmt_hours(sheet.total_seconds, sheet.lang), style=emphasis)

    if sheet.signature_lines:
        gap = 34 if sheet.signature_image is not None else 18
        if pdf.get_y() + gap + 10 > pdf.h - pdf.b_margin:
            pdf.add_page()
        pdf.ln(gap)
        y = pdf.get_y()
        pdf.set_draw_color(120, 120, 120)
        pdf.set_line_width(0.2)
        right_x = pdf.w - pdf.r_margin - 60
        pdf.line(pdf.l_margin, y, pdf.l_margin + 60, y)
        pdf.line(right_x, y, right_x + 60, y)
        if sheet.signature_image is not None:
            # With an embedded signature the date is the creation date.
            pdf.set_font("carlito", "", 10)
            pdf.set_text_color(*_INK)
            pdf.text(pdf.l_margin, y - 1.5, _fmt_date(sheet.generated, sheet.lang))
            # The baseline sits on the line; descenders cross it.
            image, baseline = _prepare_signature(sheet.signature_image)
            width = min(_SIGNATURE_H * image.width / image.height, _SIGNATURE_MAX_W)
            height = width * image.height / image.width
            x = min(right_x, pdf.w - pdf.r_margin - width)
            pdf.image(image, x=x, y=y - height * baseline, w=width, h=height)
        pdf.set_y(y + 1.5)
        pdf.set_font("carlito", "", 9)
        pdf.set_text_color(*_GRAY)
        pdf.cell(60, 5, strings["headings"]["date"])
        pdf.set_x(right_x)
        pdf.cell(60, 5, strings["signature"])

    pdf.output(str(path))
