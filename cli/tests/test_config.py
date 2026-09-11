"""Tests for the cosw config file: precedence, validation, and ``cosw config``."""

import json
from pathlib import Path

from click.testing import CliRunner, Result
from cosw.cli import main


def make_config(tmp_path: Path, text: str = "") -> Path:
    """Write a config file under a tmp XDG config home; return its path."""
    path = tmp_path / "xdg-config" / "context-switch" / "config.toml"
    path.parent.mkdir(parents=True)
    path.write_text(text)
    return path


def invoke_env(tmp_path: Path, **overrides: str | None) -> dict[str, str | None]:
    """Environment with every cosw knob pointed at tmp or cleared."""
    env: dict[str, str | None] = {
        "XDG_CONFIG_HOME": str(tmp_path / "xdg-config"),
        "XDG_DATA_HOME": str(tmp_path / "xdg-data"),
        "COSW_DATA_FILE": None,
        "COSW_CONFIG": None,
    }
    env.update(overrides)
    return env


def run(*args: str, env: dict[str, str | None]) -> Result:
    return CliRunner().invoke(main, list(args), env=env)


# --- resolution and precedence ------------------------------------------------


def test_config_file_sets_data_file(tmp_path: Path) -> None:
    target = tmp_path / "canonical.json"
    make_config(tmp_path, f'[storage]\ndata_file = "{target}"\n')
    result = run("status", "-v", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    assert f"storage: {target.resolve().as_uri()} (via config file)" in result.output


def test_flag_beats_config(tmp_path: Path) -> None:
    make_config(tmp_path, f'[storage]\ndata_file = "{tmp_path}/from-config.json"\n')
    flagged = tmp_path / "flagged.json"
    result = run("--data-file", str(flagged), "status", "-v", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    assert "(via --data-file)" in result.output
    assert flagged.exists()


def test_env_beats_config(tmp_path: Path) -> None:
    make_config(tmp_path, f'[storage]\ndata_file = "{tmp_path}/from-config.json"\n')
    from_env = tmp_path / "from-env.json"
    result = run("status", "-v", env=invoke_env(tmp_path, COSW_DATA_FILE=str(from_env)))
    assert result.exit_code == 0
    assert "(via COSW_DATA_FILE)" in result.output
    assert from_env.exists()


def test_config_beats_default(tmp_path: Path) -> None:
    target = tmp_path / "configured.json"
    make_config(tmp_path, f'[storage]\ndata_file = "{target}"\n')
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    assert target.exists()
    assert not (tmp_path / "xdg-data" / "context-switch" / "data.json").exists()


def test_missing_default_config_uses_defaults(tmp_path: Path) -> None:
    result = run("status", "-v", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    assert "(via default)" in result.output
    default_cfg = tmp_path / "xdg-config" / "context-switch" / "config.toml"
    assert f"config: {default_cfg} (missing)" in result.output


def test_verbose_json_reports_config(tmp_path: Path) -> None:
    cfg = make_config(tmp_path)
    result = run("status", "-v", "-j", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    payload = json.loads(result.output)
    assert payload["config"] == {"path": str(cfg), "exists": True}


# --- config path discovery -----------------------------------------------------


def test_cosw_config_env_var(tmp_path: Path) -> None:
    target = tmp_path / "env-located.json"
    cfg = tmp_path / "custom.toml"
    cfg.write_text(f'[storage]\ndata_file = "{target}"\n')
    result = run("status", "-v", env=invoke_env(tmp_path, COSW_CONFIG=str(cfg)))
    assert result.exit_code == 0
    assert "(via config file)" in result.output
    assert target.exists()


def test_explicit_missing_config_errors(tmp_path: Path) -> None:
    missing = tmp_path / "nope.toml"
    for args, env in (
        (["--config", str(missing), "status"], invoke_env(tmp_path)),
        (["status"], invoke_env(tmp_path, COSW_CONFIG=str(missing))),
    ):
        result = run(*args, env=env)
        assert result.exit_code != 0
        assert "config file not found" in result.output
        assert str(missing) in result.output


def test_dev_null_disables_config(tmp_path: Path) -> None:
    result = run("status", "-v", env=invoke_env(tmp_path, COSW_CONFIG="/dev/null"))
    assert result.exit_code == 0
    assert "(via default)" in result.output


# --- value semantics -----------------------------------------------------------


def test_relative_data_file_resolves_against_config_dir(tmp_path: Path) -> None:
    cfg = make_config(tmp_path, '[storage]\ndata_file = "nested/data.json"\n')
    result = run("status", "-v", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    expected = (cfg.parent / "nested" / "data.json").resolve()
    assert f"storage: {expected.as_uri()}" in result.output


def test_data_file_expands_env_vars(tmp_path: Path) -> None:
    make_config(tmp_path, '[storage]\ndata_file = "$EXTRA_DIR/data.json"\n')
    target = tmp_path / "extra"
    result = run("status", env=invoke_env(tmp_path, EXTRA_DIR=str(target)))
    assert result.exit_code == 0
    assert (target / "data.json").exists()


def test_data_file_expands_tilde(tmp_path: Path) -> None:
    make_config(tmp_path, '[storage]\ndata_file = "~/data.json"\n')
    result = run("status", env=invoke_env(tmp_path, HOME=str(tmp_path)))
    assert result.exit_code == 0
    assert (tmp_path / "data.json").exists()


# --- validation ----------------------------------------------------------------


def test_malformed_toml_errors(tmp_path: Path) -> None:
    cfg = make_config(tmp_path, "[storage\ndata_file = ")
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code != 0
    assert str(cfg) in result.output


def test_storage_must_be_a_table(tmp_path: Path) -> None:
    make_config(tmp_path, 'storage = "local"\n')
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code != 0
    assert "storage" in result.output


def test_unknown_provider_errors(tmp_path: Path) -> None:
    make_config(tmp_path, '[storage]\nprovider = "s3"\n')
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code != 0
    assert "s3" in result.output
    assert "local" in result.output


def test_data_file_must_be_a_string(tmp_path: Path) -> None:
    make_config(tmp_path, "[storage]\ndata_file = 3\n")
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code != 0
    assert "data_file" in result.output


def test_empty_data_file_errors(tmp_path: Path) -> None:
    make_config(tmp_path, '[storage]\ndata_file = ""\n')
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code != 0


def test_unknown_keys_warn_but_pass(tmp_path: Path) -> None:
    make_config(tmp_path, 'bogus = 1\n[storage]\nalso_bogus = "x"\n')
    result = run("status", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    assert "bogus" in result.stderr
    assert "also_bogus" in result.stderr


# --- cosw config command -------------------------------------------------------


def test_config_path_prints_resolved_path(tmp_path: Path) -> None:
    result = run("config", "--path", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    expected = tmp_path / "xdg-config" / "context-switch" / "config.toml"
    assert result.output.strip() == str(expected)


def test_config_path_respects_flag(tmp_path: Path) -> None:
    custom = tmp_path / "elsewhere.toml"
    result = run("--config", str(custom), "config", "--path", env=invoke_env(tmp_path))
    assert result.exit_code == 0
    assert result.output.strip() == str(custom)


def test_config_opens_editor_and_seeds_skeleton(tmp_path: Path) -> None:
    result = run("config", env=invoke_env(tmp_path, EDITOR="true"))
    assert result.exit_code == 0
    cfg = tmp_path / "xdg-config" / "context-switch" / "config.toml"
    text = cfg.read_text()
    assert "[storage]" in text
    assert 'provider = "local"' in text
    assert "# data_file" in text


def test_config_preserves_existing_file(tmp_path: Path) -> None:
    cfg = make_config(tmp_path, '[storage]\ndata_file = "/keep/me.json"\n')
    result = run("config", env=invoke_env(tmp_path, EDITOR="true"))
    assert result.exit_code == 0
    assert cfg.read_text() == '[storage]\ndata_file = "/keep/me.json"\n'


def test_config_without_editor_errors(tmp_path: Path) -> None:
    result = run("config", env=invoke_env(tmp_path, EDITOR=None, VISUAL=None))
    assert result.exit_code != 0
    assert "EDITOR" in result.output


def test_config_editor_failure_errors(tmp_path: Path) -> None:
    result = run("config", env=invoke_env(tmp_path, EDITOR="false"))
    assert result.exit_code != 0


def test_config_command_tolerates_malformed_file(tmp_path: Path) -> None:
    """A broken config must not block the command used to fix it."""
    make_config(tmp_path, "[storage\ndata_file = ")
    result = run("config", env=invoke_env(tmp_path, EDITOR="true"))
    assert result.exit_code == 0


def test_config_command_tolerates_missing_explicit_file(tmp_path: Path) -> None:
    """``cosw --config new.toml config`` seeds the file it was pointed at."""
    cfg = tmp_path / "fresh.toml"
    result = run("--config", str(cfg), "config", env=invoke_env(tmp_path, EDITOR="true"))
    assert result.exit_code == 0
    assert "[storage]" in cfg.read_text()
