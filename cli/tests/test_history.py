"""Tests for history commands: add, edit, remove."""


def _seed(invoke) -> None:
    """Two stopped spans: apollo11+module 08:00-09:30, personal 10:00-11:00."""
    invoke(
        "add",
        "apollo11",
        "+module",
        "--from",
        "2026-09-10T08:00:00Z",
        "--to",
        "2026-09-10T09:30:00Z",
    )
    invoke("add", "personal", "--from", "2026-09-10T10:00:00Z", "--to", "2026-09-10T11:00:00Z")


def test_add_records_span(invoke) -> None:
    result = invoke(
        "add",
        "apollo11",
        "+module",
        "--from",
        "2026-09-10T08:00:00Z",
        "--to",
        "2026-09-10T09:30:00Z",
    )
    assert result.exit_code == 0
    assert "created project apollo11" in result.output
    assert "Recorded apollo11 (+module)" in result.output
    assert "1h 30m" in result.output


def test_add_unassigned(invoke) -> None:
    result = invoke("add", "--from", "2026-09-10T08:00:00Z", "--to", "2026-09-10T09:00:00Z")
    assert result.exit_code == 0
    assert "unassigned" in result.output


def test_add_while_timer_active(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T12:00:00Z")
    result = invoke(
        "add", "personal", "--from", "2026-09-10T08:00:00Z", "--to", "2026-09-10T09:00:00Z"
    )
    assert result.exit_code == 0
    assert "Recorded personal" in result.output


def test_add_date_only_to_means_end_of_day(invoke) -> None:
    result = invoke("add", "apollo11", "--from", "2026-09-10", "--to", "2026-09-10")
    assert result.exit_code == 0
    assert "23h 59m" in result.output


def test_add_inverted_range_errors(invoke) -> None:
    result = invoke("add", "--from", "2026-09-10T10:00:00Z", "--to", "2026-09-10T09:00:00Z")
    assert result.exit_code == 1


def test_add_overlapping_span_errors(invoke) -> None:
    _seed(invoke)
    result = invoke(
        "add", "personal", "--from", "2026-09-10T09:00:00Z", "--to", "2026-09-10T10:30:00Z"
    )
    assert result.exit_code == 1
    assert "overlap" in result.output


def test_add_overlapping_active_timer_errors(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T12:00:00Z")
    result = invoke(
        "add", "personal", "--from", "2026-09-10T13:00:00Z", "--to", "2026-09-10T14:00:00Z"
    )
    assert result.exit_code == 1
    assert "overlap" in result.output


def test_add_invalid_to_errors(invoke) -> None:
    result = invoke("add", "--from", "2026-09-10", "--to", "bogus")
    assert result.exit_code == 2


def test_add_requires_range(invoke) -> None:
    result = invoke("add", "apollo11")
    assert result.exit_code == 2


def test_edit_times(invoke) -> None:
    _seed(invoke)
    result = invoke(
        "edit", "-1", "--start", "2026-09-10T10:15:00Z", "--stop", "2026-09-10T11:15:00Z"
    )
    assert result.exit_code == 0
    assert "Updated personal" in result.output


def test_edit_into_overlap_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "--start", "2026-09-10T09:00:00Z")
    assert result.exit_code == 1
    assert "overlap" in result.output


def test_edit_project(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "--project", "apollo11")
    assert result.exit_code == 0
    assert "Updated apollo11" in result.output


def test_edit_project_creates_unknown(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "--project", "newproj")
    assert result.exit_code == 0
    assert "created project newproj" in result.output


def test_edit_unassign(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "--unassign")
    assert result.exit_code == 0
    assert "unassigned" in result.output


def test_edit_tags(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-2", "+focus")
    assert result.exit_code == 0
    assert "created tag focus" in result.output
    assert "+focus" in result.output and "+module" in result.output
    result = invoke("edit", "-2", "--untag", "module")
    assert result.exit_code == 0
    assert "+module" not in result.output


def test_edit_remove_absent_tag_errors(invoke) -> None:
    _seed(invoke)
    invoke("tags", "add", "unused")
    result = invoke("edit", "-1", "--untag", "unused")
    assert result.exit_code == 1
    assert "no tag" in result.output


def test_edit_remove_unknown_tag_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "--untag", "nosuch")
    assert result.exit_code == 1
    assert "unknown tag" in result.output


def test_edit_nothing_to_change(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1")
    assert result.exit_code == 2
    assert "nothing to change" in result.output


def test_edit_project_and_unassign_conflict(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "--project", "x", "--unassign")
    assert result.exit_code == 2
    assert "mutually exclusive" in result.output


def test_edit_active_span_times_allowed(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z")
    result = invoke("edit", "--start", "2026-09-10T08:15:00Z")
    assert result.exit_code == 0
    assert "Updated apollo11" in result.output


def test_edit_active_span_stop_rejected(invoke) -> None:
    invoke("start", "apollo11", "--at", "2026-09-10T08:00:00Z")
    result = invoke("edit", "--stop", "2026-09-10T09:00:00Z")
    assert result.exit_code == 1


def test_edit_no_spans_errors(invoke) -> None:
    result = invoke("edit", "--project", "x")
    assert result.exit_code == 1
    assert "no spans" in result.output


def test_edit_bad_index(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-99", "--project", "x")
    assert result.exit_code == 1
    assert "no span at index" in result.output


def test_edit_zero_index(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-0", "--project", "x")
    assert result.exit_code == 1


def test_edit_by_id_prefix(invoke, data_file) -> None:
    _seed(invoke)
    listing = invoke("log")
    span_id = listing.output.split()[0]
    result = invoke("edit", span_id[:8], "--project", "apollo11")
    assert result.exit_code == 0
    assert "Updated apollo11" in result.output


def test_edit_extra_positional_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "extra", "--project", "x")
    assert result.exit_code == 2


def test_edit_bare_dash_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "-")
    assert result.exit_code == 2


def test_edit_bare_plus_errors(invoke) -> None:
    _seed(invoke)
    result = invoke("edit", "-1", "+")
    assert result.exit_code == 2


def test_write_command_on_corrupt_file(invoke, data_file) -> None:
    data_file.write_text("{ not json")
    result = invoke("stop")
    assert result.exit_code == 1
    assert "corrupt" in result.output


def test_remove_force(invoke) -> None:
    _seed(invoke)
    result = invoke("remove", "-1", "--force")
    assert result.exit_code == 0
    assert "Removed personal" in result.output
    listing = invoke("log")
    assert "personal" not in listing.output


def test_remove_confirmed(invoke) -> None:
    _seed(invoke)
    result = invoke("remove", "-1", input="y\n")
    assert result.exit_code == 0
    assert "Removed" in result.output


def test_remove_aborted_keeps_span(invoke) -> None:
    _seed(invoke)
    result = invoke("remove", "-1", input="n\n")
    assert result.exit_code != 0
    listing = invoke("log")
    assert "personal" in listing.output


def test_remove_active_span(invoke) -> None:
    invoke("start", "apollo11")
    result = invoke("remove", "-1", "--force")
    assert result.exit_code == 0
    status = invoke("status")
    assert "No active timer" in status.output


def test_remove_unknown_ref(invoke) -> None:
    _seed(invoke)
    result = invoke("remove", "deadbeef", "--force")
    assert result.exit_code == 1
    assert "no span matching" in result.output


def test_archived_project_rejected_on_start(invoke) -> None:
    invoke("projects", "add", "apollo11")
    invoke("projects", "archive", "apollo11")
    result = invoke("start", "apollo11")
    assert result.exit_code == 1
    assert "archived" in result.output
