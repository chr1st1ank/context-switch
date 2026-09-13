"""Project and tag management: ``cosw projects`` and ``cosw tags`` groups."""

from __future__ import annotations

import json
from typing import Literal

import click
from contextswitch_core import Logbook, Project, Tag

from cosw.cli import main
from cosw.core import read_logbook, require_project, require_tag, transact
from cosw.timeparse import utcnow

Kind = Literal["project", "tag"]


def _items(logbook: Logbook, kind: Kind) -> list[Project] | list[Tag]:
    return logbook.projects() if kind == "project" else logbook.tags()


def _require(logbook: Logbook, kind: Kind, name: str) -> Project | Tag:
    return require_project(logbook, name) if kind == "project" else require_tag(logbook, name)


def _add(logbook: Logbook, kind: Kind, name: str, at) -> str:
    if kind == "project":
        return logbook.add_project(name, at)
    return logbook.add_tag(name, at)


def _rename(logbook: Logbook, kind: Kind, item_id: str, name: str, at) -> None:
    if kind == "project":
        logbook.rename_project(item_id, name, at)
    else:
        logbook.rename_tag(item_id, name, at)


def _set_archived(logbook: Logbook, kind: Kind, item_id: str, archived: bool, at) -> None:
    if kind == "project":
        logbook.set_project_archived(item_id, archived, at)
    else:
        logbook.set_tag_archived(item_id, archived, at)


def _list(ctx: click.Context, kind: Kind, show_all: bool, as_json: bool) -> None:
    logbook = read_logbook(ctx)
    items = [i for i in _items(logbook, kind) if show_all or not i.archived]
    items.sort(key=lambda i: i.name.casefold())
    if as_json:
        payload = [{"id": i.id, "name": i.name, "archived": i.archived} for i in items]
        click.echo(json.dumps(payload))
        return
    if not items:
        click.echo(f"No {kind}s.")
        return
    for item in items:
        suffix = " (archived)" if item.archived else ""
        click.echo(f"{item.name}{suffix}")


def _register(group: click.Group, kind: Kind) -> None:
    @group.command("add")
    @click.argument("name")
    @click.pass_context
    def add_cmd(ctx: click.Context, name: str) -> None:
        """Create a new entry."""
        transact(ctx, lambda logbook: _add(logbook, kind, name, utcnow()))
        click.echo(f"created {kind} {name}")

    @group.command("rename")
    @click.argument("old")
    @click.argument("new")
    @click.pass_context
    def rename_cmd(ctx: click.Context, old: str, new: str) -> None:
        """Rename an entry; history keeps pointing at the same identity."""

        def mutate(logbook: Logbook) -> None:
            item = _require(logbook, kind, old)
            _rename(logbook, kind, item.id, new, utcnow())

        transact(ctx, mutate)
        click.echo(f"renamed {kind} {old} to {new}")

    @group.command("archive")
    @click.argument("name")
    @click.pass_context
    def archive_cmd(ctx: click.Context, name: str) -> None:
        """Hide an entry from pickers while preserving its history."""

        def mutate(logbook: Logbook) -> None:
            item = _require(logbook, kind, name)
            _set_archived(logbook, kind, item.id, True, utcnow())

        transact(ctx, mutate)
        click.echo(f"archived {kind} {name}")

    @group.command("unarchive")
    @click.argument("name")
    @click.pass_context
    def unarchive_cmd(ctx: click.Context, name: str) -> None:
        """Make an archived entry selectable again."""

        def mutate(logbook: Logbook) -> None:
            item = _require(logbook, kind, name)
            _set_archived(logbook, kind, item.id, False, utcnow())

        transact(ctx, mutate)
        click.echo(f"unarchived {kind} {name}")


@main.group(invoke_without_command=True)
@click.option("--all", "show_all", is_flag=True, help="Include archived entries.")
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def projects(ctx: click.Context, show_all: bool, as_json: bool) -> None:
    """List and manage projects."""
    if ctx.invoked_subcommand is None:
        _list(ctx, "project", show_all, as_json)


@main.group(invoke_without_command=True)
@click.option("--all", "show_all", is_flag=True, help="Include archived entries.")
@click.option("-j", "--json", "as_json", is_flag=True, help="Machine-readable output.")
@click.pass_context
def tags(ctx: click.Context, show_all: bool, as_json: bool) -> None:
    """List and manage tags."""
    if ctx.invoked_subcommand is None:
        _list(ctx, "tag", show_all, as_json)


_register(projects, "project")
_register(tags, "tag")
