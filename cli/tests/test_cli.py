"""Tests for the CLI entry point and scaffolding."""

import json
import runpy

import pytest
from click.testing import CliRunner
from cosw.cli import main
from cosw.core import default_data_file


def test_main_help(invoke) -> None:
    result = invoke("--help")
    assert result.exit_code == 0
    assert "context-switch time tracking CLI" in result.output
    for command in (
        "start",
        "stop",
        "switch",
        "resume",
        "cancel",
        "status",
        "add",
        "edit",
        "remove",
        "log",
        "report",
        "projects",
        "tags",
    ):
        assert command in result.output


def test_short_help_everywhere(invoke) -> None:
    for args in (
        ["-h"],
        ["start", "-h"],
        ["stop", "-h"],
        ["switch", "-h"],
        ["resume", "-h"],
        ["cancel", "-h"],
        ["status", "-h"],
        ["add", "-h"],
        ["edit", "-h"],
        ["remove", "-h"],
        ["log", "-h"],
        ["report", "-h"],
        ["projects", "-h"],
        ["projects", "add", "-h"],
        ["tags", "-h"],
        ["tags", "rename", "-h"],
    ):
        result = invoke(*args)
        assert result.exit_code == 0, args
        assert "Usage" in result.output


def test_version(invoke) -> None:
    result = invoke("--version")
    assert result.exit_code == 0
    assert "0.1.0" in result.output


def test_no_args_shows_help(invoke) -> None:
    result = invoke()
    assert "Usage" in result.output


def test_data_file_option(tmp_path) -> None:
    runner = CliRunner()
    target = tmp_path / "custom.json"
    result = runner.invoke(main, ["--data-file", str(target), "status"])
    assert result.exit_code == 0
    assert "No active timer" in result.output
    assert target.exists()


def test_data_file_env_var(tmp_path) -> None:
    runner = CliRunner()
    target = tmp_path / "env.json"
    result = runner.invoke(main, ["status"], env={"COSW_DATA_FILE": str(target)})
    assert result.exit_code == 0
    assert target.exists()


def test_default_data_file_xdg(monkeypatch, tmp_path) -> None:
    monkeypatch.delenv("COSW_DATA_FILE", raising=False)
    monkeypatch.setenv("XDG_DATA_HOME", str(tmp_path / "xdg"))
    assert default_data_file() == tmp_path / "xdg" / "context-switch" / "data.json"


def test_default_data_file_fallback(monkeypatch, tmp_path) -> None:
    monkeypatch.delenv("COSW_DATA_FILE", raising=False)
    monkeypatch.delenv("XDG_DATA_HOME", raising=False)
    assert default_data_file().name == "data.json"
    assert "context-switch" in str(default_data_file())


def test_status_json_inactive(invoke) -> None:
    result = invoke("status", "--json")
    assert result.exit_code == 0
    assert json.loads(result.output) == {"active": False}


def test_status_verbose_shows_env_source(invoke, data_file) -> None:
    result = invoke("status", "-v")
    assert result.exit_code == 0
    url = data_file.resolve().as_uri()
    assert f"storage: {url} (via COSW_DATA_FILE)" in result.output
    assert "No active timer" in result.output
    # storage line comes first, separated from the status by a blank line
    assert result.output.index("storage:") < result.output.index("No active timer")


def test_status_verbose_flag_source(tmp_path) -> None:
    runner = CliRunner()
    target = tmp_path / "flagged.json"
    result = runner.invoke(
        main, ["--data-file", str(target), "status", "-v"], env={"COSW_DATA_FILE": None}
    )
    assert result.exit_code == 0
    assert f"storage: {target.resolve().as_uri()} (via --data-file)" in result.output


def test_status_verbose_default_source(tmp_path) -> None:
    runner = CliRunner()
    xdg = tmp_path / "xdg"
    result = runner.invoke(
        main,
        ["status", "-v"],
        env={"COSW_DATA_FILE": None, "XDG_DATA_HOME": str(xdg)},
    )
    assert result.exit_code == 0
    assert "(via default)" in result.output
    assert (xdg / "context-switch" / "data.json").resolve().as_uri() in result.output


def test_status_verbose_json(invoke, data_file) -> None:
    result = invoke("status", "-v", "--json")
    assert result.exit_code == 0
    payload = json.loads(result.output)
    assert payload["active"] is False
    assert payload["storage"]["url"] == data_file.resolve().as_uri()
    assert payload["storage"]["source"] == "COSW_DATA_FILE"


def test_corrupt_data_file(invoke, data_file) -> None:
    data_file.parent.mkdir(parents=True, exist_ok=True)
    data_file.write_text("{ not json")
    result = invoke("status")
    assert result.exit_code == 1
    assert "corrupt" in result.output


def test_unwritable_data_file_path(tmp_path) -> None:
    runner = CliRunner()
    blocker = tmp_path / "blocker"
    blocker.write_text("file, not a directory")
    result = runner.invoke(main, ["status"], env={"COSW_DATA_FILE": str(blocker / "data.json")})
    assert result.exit_code == 1
    assert "Error" in result.output


def test_main_module_entrypoint() -> None:
    """Test `python -m`-style execution of cli.py."""
    from cosw import cli

    with pytest.raises(SystemExit):
        runpy.run_path(cli.__file__, run_name="__main__")


def test_python_dash_m() -> None:
    """`python -m cosw` runs the CLI."""
    with pytest.raises(SystemExit):
        runpy.run_module("cosw", run_name="__main__")
