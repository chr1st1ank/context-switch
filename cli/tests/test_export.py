"""Tests for the ``cosw export`` timesheet command."""

from datetime import UTC, datetime
from pathlib import Path

import pytest


def _seed(invoke) -> None:
    invoke(
        "add",
        "apollo11",
        "+module",
        "--from",
        "2026-09-08T08:00:00Z",
        "--to",
        "2026-09-08T09:30:00Z",
        "-d",
        "Module tests",
    )
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-09T10:00:00Z",
        "--to",
        "2026-09-09T11:00:00Z",
    )
    invoke("add", "personal", "--from", "2026-09-09T12:00:00Z", "--to", "2026-09-09T12:45:00Z")


def test_export_markdown(invoke) -> None:
    _seed(invoke)
    result = invoke("export")
    assert result.exit_code == 0
    out = result.output
    assert "# Timesheet" in out
    assert "**Name:**" in out
    assert "**Period:** 2026-09-08 – 2026-09-09" in out
    assert "| Date | Start | End | Hours | Project | Description |" in out
    assert "| apollo11 | 2.50 |" in out
    assert "| personal | 0.75 |" in out
    assert "| **Total** | **3.25** |" in out


def test_export_name(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--name", "Christian Krudewig")
    assert "**Name:** Christian Krudewig" in result.output


def test_export_period_from_filter_bounds(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--from", "2026-09-01", "--to", "2026-09-30")
    assert "**Period:** 2026-09-01 – 2026-09-30" in result.output


@pytest.mark.usefixtures("berlin_tz")
def test_export_splits_overnight_span(invoke) -> None:
    """A span crossing local midnight becomes one row per date, ending 24:00."""
    # 23:00–01:00 local in Berlin (CEST, UTC+2).
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-10T21:00:00Z",
        "--to",
        "2026-09-10T23:00:00Z",
    )
    result = invoke("export")
    assert result.exit_code == 0
    assert "| 2026-09-10 | 23:00 | 24:00 | 1.00 |" in result.output
    assert "| 2026-09-11 | 00:00 | 01:00 | 1.00 |" in result.output


def test_export_columns_selection_and_order(invoke) -> None:
    _seed(invoke)
    invoke("projects", "client", "apollo11", "NASA")
    result = invoke("export", "--columns", "project,client,tags,hours")
    assert result.exit_code == 0
    assert "| Project | Client | Tags | Hours |" in result.output
    assert "| apollo11 | NASA | module | 1.50 |" in result.output
    assert "| personal | — |  | 0.75 |" in result.output


def test_export_column_id(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--columns", "id,date,hours")
    assert result.exit_code == 0
    assert "| ID | Date | Hours |" in result.output


def test_export_unknown_column(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--columns", "date,bogus")
    assert result.exit_code == 2
    assert "unknown columns: bogus" in result.output


def test_export_bad_column_list(invoke) -> None:
    _seed(invoke)
    assert invoke("export", "--columns", "date,,hours").exit_code == 2
    assert invoke("export", "--columns", "date,date").exit_code == 2


def test_export_german(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--lang", "de")
    out = result.output
    assert "# Stundenzettel" in out
    assert "**Zeitraum:**" in out
    assert "**Erstellt am:**" in out
    assert "| Datum | Beginn | Ende | Stunden | Projekt | Beschreibung |" in out
    assert "| 08.09.2026 |" in out
    assert "1,50" in out
    assert "| **Gesamt** | **3,25** |" in out


def test_export_client_header(invoke) -> None:
    _seed(invoke)
    invoke("projects", "client", "apollo11", "NASA")
    result = invoke("export", "--client", "nasa")
    assert "**Client:** NASA" in result.output
    assert "personal" not in result.output


def test_export_no_client_header_for_multiple(invoke) -> None:
    _seed(invoke)
    invoke("projects", "client", "apollo11", "NASA")
    result = invoke("export", "--client", "nasa", "--client", "other")
    # 'other' is an unknown client -> error
    assert result.exit_code == 1
    result = invoke("export")
    assert "**Client:**" not in result.output


def test_export_unassigned_project(invoke) -> None:
    invoke("add", "--from", "2026-09-08T08:00:00Z", "--to", "2026-09-08T09:00:00Z")
    result = invoke("export")
    assert result.exit_code == 0
    assert "| — |" in result.output
    assert "|  | 1.00 |" in result.output


def test_export_description_pipe_escaped(invoke) -> None:
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-08T08:00:00Z",
        "--to",
        "2026-09-08T09:00:00Z",
        "-d",
        "a | b\nc | d",
    )
    result = invoke("export")
    assert "a \\| b c \\| d" in result.output
    assert "a | b" not in result.output


def test_export_no_spans_errors(invoke) -> None:
    result = invoke("export")
    assert result.exit_code == 1
    assert "no spans matched" in result.output


def test_export_no_spans_in_range_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--from", "2020-01-01", "--to", "2020-01-02")
    assert result.exit_code == 1


def test_export_excludes_active_timer_by_default(invoke) -> None:
    _seed(invoke)
    invoke("start", "apollo11")
    result = invoke("export")
    assert "**Total** | **3.25**" in result.output
    result = invoke("export", "-c")
    assert result.exit_code == 0
    assert result.output.count("| apollo11") > 1


def test_export_filters(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "-p", "apollo11")
    assert "personal" not in result.output
    result = invoke("export", "--ignore-project", "apollo11")
    assert "apollo11" not in result.output
    assert "| **Total** | **0.75** |" in result.output


def test_export_writes_markdown_file(invoke, tmp_path: Path) -> None:
    _seed(invoke)
    out = tmp_path / "sheet.md"
    result = invoke("export", "-o", str(out))
    assert result.exit_code == 0
    assert out.read_text().startswith("# Timesheet")


def test_export_writes_pdf(invoke, tmp_path: Path) -> None:
    _seed(invoke)
    invoke("projects", "client", "apollo11", "NASA")
    out = tmp_path / "sheet.pdf"
    result = invoke("export", "--client", "NASA", "--name", "Max Muster", "-o", str(out))
    assert result.exit_code == 0
    assert out.read_bytes().startswith(b"%PDF")


def test_export_pdf_latin1_sanitizes(invoke, tmp_path: Path) -> None:
    """Characters outside latin-1 degrade to '?' instead of crashing fpdf."""
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-08T08:00:00Z",
        "--to",
        "2026-09-08T09:00:00Z",
        "-d",
        "emoji 🚀 and — dash",
    )
    result = invoke("export", "-o", str(tmp_path / "sheet.pdf"))
    assert result.exit_code == 0


def test_export_pdf_requires_output(invoke) -> None:
    _seed(invoke)
    result = invoke("export", "--format", "pdf")
    assert result.exit_code == 2
    assert "requires -o" in result.output


def test_export_unknown_extension(invoke, tmp_path: Path) -> None:
    _seed(invoke)
    result = invoke("export", "-o", str(tmp_path / "sheet.xyz"))
    assert result.exit_code == 2
    assert "cannot infer format" in result.output


def test_export_format_flag_overrides_extension(invoke, tmp_path: Path) -> None:
    _seed(invoke)
    out = tmp_path / "sheet.txt"
    result = invoke("export", "--format", "markdown", "-o", str(out))
    assert result.exit_code == 0
    assert out.read_text().startswith("# Timesheet")


@pytest.mark.parametrize("suffix", [".md", ".pdf"])
def test_export_unwritable_output_errors(invoke, tmp_path: Path, suffix: str) -> None:
    _seed(invoke)
    result = invoke("export", "-o", str(tmp_path / "nosuchdir" / f"sheet{suffix}"))
    assert result.exit_code == 1
    assert "cannot write" in result.output


@pytest.mark.usefixtures("berlin_tz")
def test_export_golden_markdown(invoke) -> None:
    """Full-document check of the English markdown shape."""
    invoke(
        "add",
        "apollo11",
        "--from",
        "2026-09-08T08:00:00Z",
        "--to",
        "2026-09-08T09:30:00Z",
        "-d",
        "Module tests",
    )
    generated = datetime.now(UTC).astimezone().date().isoformat()
    result = invoke("export", "--name", "Christian Krudewig")
    expected = f"""# Timesheet

**Name:** Christian Krudewig
**Period:** 2026-09-08 – 2026-09-08
**Generated:** {generated}

| Date | Start | End | Hours | Project | Description |
|---|---|---|---:|---|---|
| 2026-09-08 | 10:00 | 11:30 | 1.50 | apollo11 | Module tests |

## Totals

| Project | Hours |
|---|---:|
| apollo11 | 1.50 |
| **Total** | **1.50** |
"""
    assert result.output == expected


def test_timesheet_module_is_framework_free() -> None:
    """The document module holds no CLI-framework bindings of its own."""
    import cosw.timesheet

    assert "click" not in cosw.timesheet.__dict__
