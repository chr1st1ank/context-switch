"""Shell completion: script generation, name cache, and dynamic completers."""

import stat
from collections.abc import Callable
from pathlib import Path

import pytest
from click.shell_completion import get_completion_class
from click.testing import Result
from cosw.cli import main


def complete(*words: str, incomplete: str = "") -> list[str]:
    shell = get_completion_class("bash")
    assert shell is not None
    comp = shell(main, {}, "cosw", "_COSW_COMPLETE")
    return [item.value for item in comp.get_completions(list(words), incomplete)]


@pytest.fixture
def populated(invoke: Callable[..., Result], data_file: Path) -> Path:
    for args in (
        ("start", "Alpha", "+deep", "+review"),
        ("switch", "Beta", "+deep"),
        ("stop",),
        ("projects", "add", "Gamma"),
        ("projects", "archive", "Gamma"),
        ("tags", "add", "old"),
        ("tags", "archive", "old"),
    ):
        result = invoke(*args)
        assert result.exit_code == 0, result.output
    return data_file


@pytest.fixture(autouse=True)
def _data_env(monkeypatch: pytest.MonkeyPatch, data_file: Path) -> None:
    monkeypatch.setenv("COSW_DATA_FILE", str(data_file))


@pytest.mark.parametrize("shell", ["bash", "zsh", "fish"])
def test_completion_script_registers_both_entry_points(
    invoke: Callable[..., Result], shell: str
) -> None:
    result = invoke("completion", shell)
    assert result.exit_code == 0, result.output
    assert "_COSW_COMPLETE" in result.output
    assert "_CONTEXTSWITCH_COMPLETE" in result.output


def test_completion_rejects_unknown_shell(invoke: Callable[..., Result]) -> None:
    assert invoke("completion", "powershell").exit_code != 0


def test_completion_script_works_with_broken_config(
    invoke: Callable[..., Result], tmp_path: Path
) -> None:
    config = tmp_path / "broken.toml"
    config.write_text("not [valid")
    result = invoke("--config", str(config), "completion", "bash")
    assert result.exit_code == 0, result.output


def test_static_completion_of_subcommands() -> None:
    assert {"start", "status", "stop"} <= set(complete(incomplete="st"))
    assert "completion" in complete(incomplete="comp")


def test_completion_choices_for_shell() -> None:
    assert complete("completion", incomplete="") == ["bash", "zsh", "fish"]


def test_projects_complete_for_first_positional(populated: Path) -> None:
    assert complete("start", incomplete="") == ["Alpha", "Beta"]
    assert complete("switch", incomplete="al") == ["Alpha"]


def test_tags_complete_with_plus_prefix(populated: Path) -> None:
    assert complete("start", "Alpha", incomplete="+") == ["+deep", "+review"]
    assert complete("start", incomplete="+d") == ["+deep"]


def test_no_project_offered_after_project_given(populated: Path) -> None:
    assert complete("start", "Alpha", incomplete="") == []


def test_history_add_completes_classification(populated: Path) -> None:
    assert complete("add", incomplete="B") == ["Beta"]


def test_edit_completes_only_tags_positionally(populated: Path) -> None:
    assert complete("edit", incomplete="") == []
    assert complete("edit", incomplete="+r") == ["+review"]


def test_option_values_complete(populated: Path) -> None:
    assert complete("edit", "--project", incomplete="A") == ["Alpha"]
    assert complete("edit", "--untag", incomplete="d") == ["deep"]
    assert complete("log", "-p", incomplete="B") == ["Beta"]
    assert complete("report", "--ignore-project", incomplete="A") == ["Alpha"]
    assert complete("log", "-T", incomplete="r") == ["review"]
    assert complete("log", "--ignore-tag", incomplete="d") == ["deep"]


def test_manage_commands_complete_names(populated: Path) -> None:
    assert complete("projects", "rename", incomplete="") == ["Alpha", "Beta"]
    assert complete("projects", "rename", "Alpha", incomplete="") == []
    assert complete("projects", "archive", incomplete="A") == ["Alpha"]
    assert complete("projects", "client", incomplete="B") == ["Beta"]
    assert complete("tags", "archive", incomplete="") == ["deep", "review"]
    assert complete("projects", "add", incomplete="") == []


def test_unarchive_offers_archived_names_only(populated: Path) -> None:
    assert complete("projects", "unarchive", incomplete="") == ["Gamma"]
    assert complete("tags", "unarchive", incomplete="") == ["old"]


def test_cache_file_is_private(populated: Path, tmp_path: Path) -> None:
    files = list((tmp_path / "xdg-cache").rglob("*.json"))
    assert len(files) == 1
    assert stat.S_IMODE(files[0].stat().st_mode) == 0o600


def test_cache_is_refreshed_by_read_only_commands(
    populated: Path, invoke: Callable[..., Result], tmp_path: Path
) -> None:
    for f in (tmp_path / "xdg-cache").rglob("*.json"):
        f.unlink()
    assert complete("start", incomplete="") == []
    assert invoke("status").exit_code == 0
    assert complete("start", incomplete="") == ["Alpha", "Beta"]


def test_cache_is_scoped_per_storage_location(
    populated: Path, invoke: Callable[..., Result], tmp_path: Path
) -> None:
    other = tmp_path / "other.json"
    assert invoke("--data-file", str(other), "start", "Other").exit_code == 0
    assert complete("--data-file", str(other), "start", incomplete="") == ["Other"]
    assert complete("start", incomplete="") == ["Alpha", "Beta"]


def test_missing_cache_yields_nothing() -> None:
    assert complete("start", incomplete="") == []


def test_corrupt_cache_yields_nothing(populated: Path, tmp_path: Path) -> None:
    for f in (tmp_path / "xdg-cache").rglob("*.json"):
        f.write_text("{not json")
    assert complete("start", incomplete="") == []


def test_wrongly_shaped_cache_yields_nothing(populated: Path, tmp_path: Path) -> None:
    for f in (tmp_path / "xdg-cache").rglob("*.json"):
        f.write_text('{"project": 3}')
    assert complete("start", incomplete="") == []


def test_broken_config_yields_nothing(tmp_path: Path, capfd: pytest.CaptureFixture[str]) -> None:
    config = tmp_path / "broken.toml"
    config.write_text("not [valid")
    assert complete("--config", str(config), "start", incomplete="") == []
    assert capfd.readouterr().err == ""


def test_unknown_config_key_warning_is_suppressed(
    populated: Path, tmp_path: Path, capfd: pytest.CaptureFixture[str]
) -> None:
    config = tmp_path / "config.toml"
    config.write_text("bogus = 1\n")
    complete("--config", str(config), "start", incomplete="")
    assert capfd.readouterr().err == ""


def test_unwritable_cache_does_not_fail_commands(
    invoke: Callable[..., Result], tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    blocker = tmp_path / "blocker"
    blocker.write_text("")
    monkeypatch.setenv("XDG_CACHE_HOME", str(blocker))
    assert invoke("start", "Alpha").exit_code == 0
    assert invoke("status").exit_code == 0


def test_s3_cache_key_needs_no_passphrase(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> None:
    config = tmp_path / "s3.toml"
    config.write_text(
        '[storage]\nprovider = "s3"\nbucket = "b"\nregion = "r"\npassphrase_command = "false"\n'
    )
    from cosw.config import load_config
    from cosw.namecache import cache_file, storage_key

    cfg = load_config(config, explicit=True)
    key = storage_key(cfg, tmp_path / "unused.json")
    assert key.startswith("s3://b/")
    assert cache_file(key).parent == tmp_path / "xdg-cache" / "context-switch"
    assert complete("--config", str(config), "start", incomplete="") == []
