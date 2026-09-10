"""Tests for the CLI entry point."""

import runpy

import pytest
from click.testing import CliRunner
from cosw.cli import main


def test_main_help() -> None:
    """Test that the main command shows help."""
    runner = CliRunner()
    result = runner.invoke(main, ["--help"])
    assert result.exit_code == 0
    assert "context-switch time tracking CLI" in result.output


def test_start_command(sample_project: str) -> None:
    """Test the start command."""
    runner = CliRunner()
    result = runner.invoke(main, ["start", "--project", sample_project])
    assert result.exit_code == 0
    assert "Starting timer" in result.output


def test_start_command_with_tags() -> None:
    """Test that start echoes the given tags."""
    runner = CliRunner()
    result = runner.invoke(main, ["start", "--tags", "a", "--tags", "b"])
    assert result.exit_code == 0
    assert "Tags: a, b" in result.output


def test_stop_command() -> None:
    """Test the stop command."""
    runner = CliRunner()
    result = runner.invoke(main, ["stop"])
    assert result.exit_code == 0
    assert "Stopping" in result.output


def test_switch_command() -> None:
    """Test the switch command."""
    runner = CliRunner()
    result = runner.invoke(main, ["switch", "--project", "next", "--tags", "x"])
    assert result.exit_code == 0
    assert "Switching to project: next" in result.output
    assert "Tags: x" in result.output


def test_status_command() -> None:
    """Test the status command."""
    runner = CliRunner()
    result = runner.invoke(main, ["status"])
    assert result.exit_code == 0
    assert "not implemented" in result.output


def test_main_module_entrypoint() -> None:
    """Test `python -m`-style execution of cli.py."""
    from cosw import cli

    with pytest.raises(SystemExit) as exc_info:
        runpy.run_path(cli.__file__, run_name="__main__")
    assert exc_info.value.code == 2
