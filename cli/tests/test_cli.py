"""Tests for the CLI entry point."""

from click.testing import CliRunner

from cosw.cli import main


def test_main_help() -> None:
    """Test that the main command shows help."""
    runner = CliRunner()
    result = runner.invoke(main, ["--help"])
    assert result.exit_code == 0
    assert "context-switch time tracking CLI" in result.output


def test_start_command() -> None:
    """Test the start command."""
    runner = CliRunner()
    result = runner.invoke(main, ["start", "--project", "test"])
    assert result.exit_code == 0
    assert "Starting timer" in result.output


def test_stop_command() -> None:
    """Test the stop command."""
    runner = CliRunner()
    result = runner.invoke(main, ["stop"])
    assert result.exit_code == 0
    assert "Stopping" in result.output
