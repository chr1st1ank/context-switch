"""Main CLI entry point."""

import click


@click.group()
@click.version_option()
def main() -> None:
    """context-switch time tracking CLI."""
    pass


@main.command()
@click.option("--project", help="Project name for the new timer")
@click.option("--tags", multiple=True, help="Tags to attach to the timer")
def start(project: str | None, tags: tuple[str, ...]) -> None:
    """Start a new timer."""
    click.echo(f"Starting timer for project: {project}")
    if tags:
        click.echo(f"Tags: {', '.join(tags)}")


@main.command()
def stop() -> None:
    """Stop the active timer."""
    click.echo("Stopping active timer")


@main.command()
@click.option("--project", help="Project name for the new timer")
@click.option("--tags", multiple=True, help="Tags to attach to the timer")
def switch(project: str | None, tags: tuple[str, ...]) -> None:
    """Stop the active timer and start a new one."""
    click.echo(f"Switching to project: {project}")
    if tags:
        click.echo(f"Tags: {', '.join(tags)}")


@main.command()
def status() -> None:
    """Show the current timer status."""
    click.echo("Timer status: not implemented")


if __name__ == "__main__":
    main()
